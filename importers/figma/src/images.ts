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

function isPng(bytes: Uint8Array): boolean {
  return bytes.length >= PNG_SIGNATURE.length &&
    PNG_SIGNATURE.every((byte, at) => bytes[at] === byte);
}

/**
 * Downloads the bytes behind each ref.
 *
 * @throws when a ref has no URL, a download fails, or an asset is not a PNG —
 * the `.dsb` image table knows exactly one container format in v0.3, and
 * guessing at the rest is what P4 forbids.
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
    if (!isPng(bytes)) {
      throw new Error(
        `figma-image-format: the asset for imageRef ${ref} is not a PNG — ` +
          "the v0.3 image table carries PNG only (dashpaint::ImageFormat)",
      );
    }

    images.set(ref, { format: "png", bytes });
  }

  return images;
}
