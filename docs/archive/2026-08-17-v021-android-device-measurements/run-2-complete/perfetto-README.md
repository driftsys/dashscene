# Taking a Perfetto trace

The configuration beside this file is committed at
`measure/android/perfetto-frames.pbtx`, so a trace is a named command rather than
an argument list assembled at the device.

    adb push measure/android/perfetto-frames.pbtx /data/misc/perfetto-configs/
    adb shell perfetto --txt -c /data/misc/perfetto-configs/perfetto-frames.pbtx \
        -o /data/misc/perfetto-traces/dashscene.perfetto-trace
    adb pull /data/misc/perfetto-traces/dashscene.perfetto-trace

Open it at ui.perfetto.dev. What it holds and what it does not is in
the configuration's own comments.

**Vendor GPU counters are deliberately not in it.** The counter names
differ between Adreno, Mali and PowerVR, and the adapter is not known
until `just android-probe` reports it on the device. Add them in a
second pass, once the probe has named the GPU:

    adb shell perfetto --query | head -60

lists the data sources that device actually has, including
`gpu.counters` and its available counter ids when the vendor exposes
them.
