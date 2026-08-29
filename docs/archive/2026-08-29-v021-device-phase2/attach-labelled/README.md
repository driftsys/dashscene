# The two launch conditions, measured against each other (#960)

    taken     2026-08-29 on a Pixel 5 (redfin), Android 14 / API 34, Adreno 620
    from      `./measure/android/attach-timing.sh target/attach-labelled`
    device    11181FDD4002MY

**Why this run exists.** `docs/design/android-toolchain.md` explained a
fifteen-fold spread between two debug attaches as a first-launch-after-install
effect. Every row it was explaining was a first launch after install —
`attach-timing.sh` uninstalled and installed before every profile,
unconditionally — so the condition could not account for a difference within the
table. This is the run that measures the condition instead of arguing from it.

`attach-timing.sh` now installs once per profile and launches twice, so a
`later` row is the same installed build launched again with no reinstall
between. Four captures, one per row, are beside this file.

## The result

    profile   launch                acquire   to first frame   TotalTime
    release   first-after-install   0.31 s    0.34 s           205 ms
    release   later                 0.26 s    0.28 s           179 ms
    debug     first-after-install   0.73 s    0.97 s           203 ms
    debug     later                 0.68 s    0.91 s           182 ms

**The first-launch premium is about 60 ms in both profiles.** The difference it
was offered to explain was 13.64 s. It is smaller by a factor of about 230.

**The debug penalty is 3.3x like for like** — 0.91 s against 0.28 s, both later
launches.

## What this does not settle

Run 1 of 2026-08-17 took 14.57 s to a first frame and nothing here explains it.
What this run removes is one candidate explanation, measured rather than
reasoned about. Five debug attempts have now been made on this device: 14.57,
0.93, 0.95, 0.97 and 0.91 s.
