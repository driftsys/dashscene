/**
 * imageRef → bytes.
 *
 * This is the seam story #139 pinned: `dashc` compiles to
 * wasm32-unknown-unknown, so it does no network and no filesystem I/O — and
 * Figma serializes an image fill as a bare `imageRef` with no bytes anywhere in
 * the file JSON. Whoever *can* fetch resolves the refs and hands the bytes
 * across the ABI. That is this file.
 *
 * Which refs to resolve is not decided here either: `dashc` is asked
 * (`figmaImageRefs`), because it is the module that consumes them.
 */

import type { FigmaClient } from "./fetch.ts";
import type { ImageAsset } from "./wasm.ts";

export interface ResolveImagesOptions {
  readonly client: FigmaClient;
  readonly fileKey: string;
  /** The refs the lowering demands — from `Dashc.figmaImageRefs`. */
  readonly refs: readonly string[];
  /**
   * Injectable for tests. Defaults to the global fetch.
   *
   * The asset download does not go to `api.figma.com` — the URLs are presigned
   * and point at Figma's asset host — so it does not run through the REST
   * client's limiter, which exists for the rate-limited API (§11).
   */
  readonly fetchFn?: typeof fetch;
}

/** The eight bytes that open every PNG (RFC 2083 §3.1). */
const PNG_SIGNATURE = Uint8Array.from([
  0x89,
  0x50,
  0x4e,
  0x47,
  0x0d,
  0x0a,
  0x1a,
  0x0a,
]);

/** Whether `bytes` opens with the eight-byte PNG signature. */
export function isPng(bytes: Uint8Array): boolean {
  return bytes.length >= PNG_SIGNATURE.length &&
    PNG_SIGNATURE.every((byte, at) => bytes[at] === byte);
}

/**
 * The three bytes that open every JPEG/JFIF stream: the SOI marker (FF D8)
 * followed by the FF of the first marker that always follows it.
 */
const JPEG_SIGNATURE = Uint8Array.from([0xff, 0xd8, 0xff]);

/** Whether `bytes` opens with the three-byte JPEG start-of-image signature. */
export function isJpeg(bytes: Uint8Array): boolean {
  return bytes.length >= JPEG_SIGNATURE.length &&
    JPEG_SIGNATURE.every((byte, at) => bytes[at] === byte);
}

/** The two bytes every JPEG stream ends with: the End Of Image marker. */
const JPEG_EOI = Uint8Array.from([0xff, 0xd9]);

/**
 * Whether a JPEG stream (already confirmed by {@linkcode isJpeg}) is long
 * enough to plausibly be real and ends with the End Of Image marker. Not a
 * full JPEG parse — just enough to refuse an obviously truncated download
 * rather than silently accept it (P4): the SOI signature alone matches the
 * first three bytes of any download cut short mid-transfer.
 */
function isWellFormedJpeg(bytes: Uint8Array): boolean {
  const JPEG_MINIMUM_LENGTH = 8;
  return bytes.length >= JPEG_MINIMUM_LENGTH &&
    bytes[bytes.length - 2] === JPEG_EOI[0] &&
    bytes[bytes.length - 1] === JPEG_EOI[1];
}

/**
 * The four bytes every GIF stream opens with ("GIF8"), common to both the
 * GIF87a and GIF89a header versions (GIF89a §17).
 */
const GIF_SIGNATURE = Uint8Array.from([0x47, 0x49, 0x46, 0x38]);

/** Whether `bytes` opens with the four-byte GIF signature. */
export function isGif(bytes: Uint8Array): boolean {
  return bytes.length >= GIF_SIGNATURE.length &&
    GIF_SIGNATURE.every((byte, at) => bytes[at] === byte);
}

/**
 * "NETSCAPE2.0" — the Application Extension identifier GIF encoders write
 * to carry a loop count. GIF89a leaves looping unspecified (§26 covers only
 * the plain Application Extension envelope); this identifier is the de
 * facto standard every encoder and browser has used since Netscape 2.0.
 */
const NETSCAPE_APPLICATION_ID = Uint8Array.from([
  0x4e,
  0x45,
  0x54,
  0x53,
  0x43,
  0x41,
  0x50,
  0x45,
  0x32,
  0x2e,
  0x30,
]);

/**
 * One structurally valid GIF's shape: how many Image Descriptor blocks it
 * carries, and whether it carries a NETSCAPE2.0 Application Extension (the
 * loop signal) — GIF89a §18-27's two animation signals.
 */
interface GifStructure {
  readonly imageCount: number;
  readonly netscapeLoop: boolean;
}

/**
 * Walks a GIF stream's block structure (GIF89a §18-27) and returns its
 * shape, or `null` when the stream runs out of bytes — or otherwise
 * disagrees with GIF89a's grammar — before a well-formed structure
 * completes (a Trailer reached, with at least one Image Descriptor along
 * the way).
 *
 * A magic-byte match alone does not mean a valid container: a download cut
 * short mid-transfer still opens with "GIF8", and silently accepting it as
 * a zero-frame "static" image is exactly the guess P4 forbids —
 * {@linkcode classify} refuses it by name instead.
 *
 * Never scans for byte values to find the next block — LZW-compressed
 * image data can contain any byte, including the Image Separator's own
 * 0x2C — every jump is computed from a declared length, and bounds-checked
 * before it is taken.
 */
function walkGif(bytes: Uint8Array): GifStructure | null {
  const HEADER_AND_SCREEN_DESCRIPTOR = 13;
  if (bytes.length < HEADER_AND_SCREEN_DESCRIPTOR) return null;

  // Header (6 bytes) then the Logical Screen Descriptor's packed byte, two
  // bytes into its own 7-byte field.
  let at = 6;
  const screenPacked = bytes[10];
  at += 7;
  if ((screenPacked & 0x80) !== 0) {
    // Global Color Table present; its size is 3 * 2^(N+1) bytes.
    at += 3 * 2 ** ((screenPacked & 0x07) + 1);
  }
  if (at > bytes.length) return null; // the Global Color Table itself is truncated.

  /** Walks one size-prefixed sub-block chain; `false` if it runs off the end. */
  const skipSubBlocks = (): boolean => {
    while (at < bytes.length) {
      const size = bytes[at];
      at += 1;
      if (size === 0) return true;
      at += size;
      if (at > bytes.length) return false;
    }
    return false; // ran out of bytes before the terminating zero-size block.
  };

  let imageCount = 0;
  let netscapeLoop = false;
  let sawTrailer = false;

  while (at < bytes.length) {
    const tag = bytes[at];
    at += 1;

    if (tag === 0x3b) {
      sawTrailer = true;
      break;
    }

    if (tag === 0x21) {
      // Extension Introducer; the next byte names which extension. Every
      // extension's payload — Application included — is a size-prefixed
      // sub-block chain from here, so one walker clears all of them.
      if (at >= bytes.length) return null;
      const label = bytes[at];
      at += 1;
      const idStart = at + 1;
      if (
        label === 0xff && bytes[at] === NETSCAPE_APPLICATION_ID.length &&
        idStart + NETSCAPE_APPLICATION_ID.length <= bytes.length &&
        NETSCAPE_APPLICATION_ID.every((byte, i) => bytes[idStart + i] === byte)
      ) {
        netscapeLoop = true;
      }
      if (!skipSubBlocks()) return null;
      continue;
    }

    if (tag === 0x2c) {
      // Image Descriptor: Left/Top/Width/Height (four u16) then Packed.
      if (at + 9 > bytes.length) return null;
      imageCount += 1;
      at += 8;
      const imagePacked = bytes[at];
      at += 1;
      if ((imagePacked & 0x80) !== 0) {
        // Local Color Table present; same size formula as the global one.
        at += 3 * 2 ** ((imagePacked & 0x07) + 1);
        if (at > bytes.length) return null;
      }
      if (at >= bytes.length) return null; // the LZW minimum code size byte.
      at += 1;
      if (!skipSubBlocks()) return null;
      continue;
    }

    // An unrecognized block tag means the stream disagrees with GIF89a's
    // grammar — not a well-formed GIF.
    return null;
  }

  if (!sawTrailer || imageCount < 1) return null;

  return { imageCount, netscapeLoop };
}

/**
 * Whether a GIF stream (already confirmed by {@linkcode isGif}) is
 * animated: more than one Image Descriptor block, or a NETSCAPE2.0
 * Application Extension (the loop signal).
 *
 * A malformed or truncated stream is not "animated" by this predicate's own
 * contract — false, not a guess. {@linkcode classify} calls
 * {@linkcode walkGif} directly to refuse those by name instead of reaching
 * this function at all.
 */
export function isAnimatedGif(bytes: Uint8Array): boolean {
  const structure = walkGif(bytes);
  return structure !== null &&
    (structure.imageCount > 1 || structure.netscapeLoop);
}

/** The refusal for a container that matches a signature but is not, on
 * closer inspection, a well-formed instance of it — a truncated download,
 * for instance. Named once so every "close but not it" case reads the same
 * way as the "no signature at all" case (both are `figma-image-format`). */
function notARecognizedContainer(ref: string): Error {
  return new Error(
    `figma-image-format: the asset for imageRef ${ref} is not a recognized ` +
      "container — the image table carries PNG, JPEG, and static GIF " +
      "(dashpaint::ImageFormat)",
  );
}

/**
 * Classifies a downloaded asset by its magic bytes and tags it — never by
 * guessing from a URL, a `Content-Type`, or the Figma API's own metadata,
 * per P4.
 *
 * @throws when the bytes match no recognized container, match a
 * signature but are truncated or otherwise malformed, or match a GIF that
 * is animated (multi-frame or NETSCAPE-looping) — the v0.10 image table
 * carries PNG, JPEG, and static GIF only; animated GIF content is a
 * separate, not-yet-decided v0.11+ question.
 */
function classify(bytes: Uint8Array, ref: string): ImageAsset["format"] {
  if (isPng(bytes)) return "png";

  if (isJpeg(bytes)) {
    if (!isWellFormedJpeg(bytes)) throw notARecognizedContainer(ref);
    return "jpeg";
  }

  if (isGif(bytes)) {
    const gif = walkGif(bytes);
    if (gif === null) throw notARecognizedContainer(ref);
    if (gif.imageCount > 1 || gif.netscapeLoop) {
      throw new Error(
        `figma-image-animated-gif: the asset for imageRef ${ref} is an ` +
          "animated GIF — the image table carries static (single-frame) " +
          "GIF only; animated GIF content is out of scope",
      );
    }
    return "gif";
  }

  throw notARecognizedContainer(ref);
}

/**
 * Downloads the bytes behind each ref.
 *
 * @throws when a ref has no URL, a download fails, or an asset is not a
 * recognized static container (see {@linkcode classify}) — guessing at an
 * unrecognized or animated container is what P4 forbids.
 */
export async function resolveImages(
  options: ResolveImagesOptions,
): Promise<Map<string, ImageAsset>> {
  const { client, fileKey, refs } = options;
  const images = new Map<string, ImageAsset>();
  if (refs.length === 0) return images;

  const fetchFn = options.fetchFn ?? fetch;
  const urls = await client.imageFills(fileKey);

  for (const ref of refs) {
    const url = urls[ref];
    if (!url) {
      throw new Error(
        "figma-image-unresolved: the file's image map has no URL for " +
          `imageRef ${ref} — the fill references an asset the file does ` +
          "not carry",
      );
    }

    const response = await fetchFn(url);
    if (!response.ok) {
      await response.body?.cancel();
      throw new Error(
        `figma-image-download: GET the asset for imageRef ${ref} returned ` +
          `${response.status}`,
      );
    }

    const bytes = new Uint8Array(await response.arrayBuffer());
    const format = classify(bytes, ref);

    images.set(ref, { format, bytes });
  }

  return images;
}
