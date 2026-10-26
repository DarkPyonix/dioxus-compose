## Window runs (Darwin)

### idle-top

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| idle | generated (100000 lines) | 4.50 / 8.00 / 10.51 (n=120) | 16.38 / 19.82 / 20.61 (n=120) | 11.89 / 15.65 / 16.37 (n=120) | 16.38 / 19.82 / 20.60 (n=120) | - | - |

    editor-spike window: idle | generated (100000 lines) | scroll top | frames served 182 | rebuild 30.79 ms
    settling frames: interval 16.81 / 30.36 / 411.57 (n=61) | event renders: 4.56 / 4.56 / 4.56 (n=1) | handle_event: -

```
2026-10-26 07:38:08.490 editor-window[10150:31594] compose-rust: parity system_theme light
2026-10-26 07:38:08.490 editor-window[10150:31594] compose-rust: latency frames=182 average=5.74ms worst=50.92ms
2026-10-26 07:38:08.490 editor-window[10150:31594] compose-rust: parity frame_idle_avg_ms 5.74 frames=182 worst_ms=50.92
exit status 0
```

### page-top

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| page | generated (100000 lines) | 34.04 / 67.03 / 78.00 (n=100) | 58.36 / 92.70 / 106.73 (n=100) | 24.27 / 28.09 / 32.97 (n=100) | 58.36 / 92.68 / 106.76 (n=100) | - | - |

    editor-spike window: page | generated (100000 lines) | scroll top | frames served 162 | rebuild 30.27 ms
    settling frames: interval 16.73 / 40.56 / 84.48 (n=61) | event renders: 5.13 / 5.13 / 5.13 (n=1) | handle_event: -

```
2026-10-26 07:38:39.147 editor-window[11009:33693] compose-rust: parity system_theme light
2026-10-26 07:38:39.147 editor-window[11009:33693] compose-rust: latency frames=163 average=38.91ms worst=119.03ms
2026-10-26 07:38:39.147 editor-window[11009:33693] compose-rust: parity frame_idle_avg_ms 38.91 frames=163 worst_ms=119.03
exit status 0
```

### realtype-top

The probe did not report: see realtype-top.log.

```
editor-spike event: render 10.264 ms
editor-spike event: handle_event 0.090 ms (node 319, handler 2)
editor-spike event: render 21.255 ms
editor-spike event: handle_event 0.047 ms (node 319, handler 2)
editor-spike event: render 19.520 ms
editor-spike event: handle_event 0.031 ms (node 319, handler 2)
editor-spike event: render 14.620 ms
editor-spike event: handle_event 0.040 ms (node 319, handler 2)
editor-spike event: render 15.819 ms
editor-spike event: handle_event 0.027 ms (node 319, handler 2)
editor-spike event: render 6.606 ms
2026-10-26 07:45:27.149 editor-window[22028:58771] compose-rust: parity system_theme light
2026-10-26 07:45:27.149 editor-window[22028:58771] compose-rust: latency frames=14 average=12.97ms worst=30.82ms
2026-10-26 07:45:27.150 editor-window[22028:58771] compose-rust: parity frame_idle_avg_ms 12.97 frames=14 worst_ms=30.82
2026-10-26 07:45:27.150 editor-window[22028:58771] compose-rust: parity input_latency_avg_ms 23.05 samples=5 worst_ms=32.38
exit status 0
```

### type-real

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| type | stylist.rs (4465 lines) | 7.43 / 11.84 / 32.85 (n=1000) | 16.65 / 20.18 / 47.94 (n=1000) | 9.04 / 15.25 / 36.19 (n=1000) | 16.66 / 20.18 / 47.95 (n=1000) | 9.23 / 13.19 / 32.85 (n=500) | 15.66 / 20.47 / 47.94 (n=500) |

    editor-spike window: type | stylist.rs (4465 lines) | scroll top | frames served 1062 | rebuild 12.44 ms
    settling frames: interval 16.77 / 35.63 / 199.06 (n=61) | event renders: 5.83 / 5.83 / 5.83 (n=1) | handle_event: -

```
2026-10-26 07:45:14.439 editor-window[19726:53648] compose-rust: parity system_theme light
2026-10-26 07:45:14.439 editor-window[19726:53648] compose-rust: latency frames=1063 average=9.15ms worst=49.07ms
2026-10-26 07:45:14.439 editor-window[19726:53648] compose-rust: parity frame_idle_avg_ms 9.15 frames=1063 worst_ms=49.07
exit status 0
```

### type-top

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| type | generated (100000 lines) | 8.00 / 14.40 / 34.92 (n=1000) | 16.71 / 21.54 / 50.73 (n=1000) | 7.24 / 15.02 / 38.91 (n=1000) | 16.71 / 21.55 / 50.74 (n=1000) | 10.18 / 16.26 / 34.92 (n=500) | 16.14 / 25.47 / 50.73 (n=500) |

    editor-spike window: type | generated (100000 lines) | scroll top | frames served 1062 | rebuild 11.82 ms
    settling frames: interval 16.72 / 45.93 / 94.54 (n=61) | event renders: 4.71 / 4.71 / 4.71 (n=1) | handle_event: -

```
2026-10-26 07:40:50.598 editor-window[12772:37847] compose-rust: parity system_theme light
2026-10-26 07:40:50.598 editor-window[12772:37847] compose-rust: latency frames=1063 average=10.09ms worst=46.77ms
2026-10-26 07:40:50.598 editor-window[12772:37847] compose-rust: parity frame_idle_avg_ms 10.09 frames=1063 worst_ms=46.77
exit status 0
```

### type-transform

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| type | generated (100000 lines) | 9.10 / 15.38 / 30.86 (n=1000) | 16.63 / 23.62 / 78.30 (n=1000) | 6.65 / 14.78 / 61.34 (n=1000) | 16.63 / 23.57 / 78.28 (n=1000) | 10.83 / 17.66 / 30.86 (n=500) | 16.29 / 26.65 / 78.30 (n=500) |

    editor-spike window: type | generated (100000 lines) | scroll transform | frames served 1062 | rebuild 15.28 ms
    settling frames: interval 16.91 / 67.53 / 164.67 (n=61) | event renders: 6.45 / 6.45 / 6.45 (n=1) | handle_event: -

```
2026-10-26 07:43:02.153 editor-window[16239:45779] compose-rust: parity system_theme light
2026-10-26 07:43:02.154 editor-window[16239:45779] compose-rust: latency frames=1063 average=11.02ms worst=75.61ms
2026-10-26 07:43:02.154 editor-window[16239:45779] compose-rust: parity frame_idle_avg_ms 11.02 frames=1063 worst_ms=75.61
exit status 0
```

### wheel-real

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| wheel | stylist.rs (4465 lines) | 8.02 / 14.80 / 35.14 (n=300) | 31.29 / 49.82 / 84.27 (n=300) | 22.77 / 36.31 / 55.50 (n=300) | 31.30 / 49.83 / 84.39 (n=300) | - | - |

    editor-spike window: wheel | stylist.rs (4465 lines) | scroll top | frames served 362 | rebuild 17.70 ms
    settling frames: interval 17.00 / 46.71 / 116.53 (n=61) | event renders: 5.31 / 5.31 / 5.31 (n=1) | handle_event: -

```
2026-10-26 07:43:43.229 editor-window[18541:50970] compose-rust: parity system_theme light
2026-10-26 07:43:43.229 editor-window[18541:50970] compose-rust: latency frames=363 average=22.02ms worst=81.19ms
2026-10-26 07:43:43.229 editor-window[18541:50970] compose-rust: parity frame_idle_avg_ms 22.02 frames=363 worst_ms=81.19
exit status 0
```

### wheel-top

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| wheel | generated (100000 lines) | 12.08 / 19.55 / 28.67 (n=300) | 39.42 / 57.66 / 93.14 (n=300) | 27.67 / 41.36 / 67.61 (n=300) | 39.43 / 57.65 / 93.12 (n=300) | - | - |

    editor-spike window: wheel | generated (100000 lines) | scroll top | frames served 362 | rebuild 14.95 ms
    settling frames: interval 16.69 / 66.92 / 104.58 (n=61) | event renders: 7.28 / 7.28 / 7.28 (n=1) | handle_event: -

```
2026-10-26 07:39:19.747 editor-window[11601:35155] compose-rust: parity system_theme light
2026-10-26 07:39:19.747 editor-window[11601:35155] compose-rust: latency frames=363 average=28.27ms worst=97.45ms
2026-10-26 07:39:19.748 editor-window[11601:35155] compose-rust: parity frame_idle_avg_ms 28.27 frames=363 worst_ms=97.45
exit status 0
```

### wheel-transform

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| wheel | generated (100000 lines) | 9.92 / 17.10 / 31.95 (n=300) | 34.00 / 52.40 / 78.59 (n=300) | 24.20 / 37.50 / 61.07 (n=300) | 34.04 / 52.46 / 78.60 (n=300) | - | - |

    editor-spike window: wheel | generated (100000 lines) | scroll transform | frames served 362 | rebuild 16.22 ms
    settling frames: interval 16.90 / 83.80 / 158.90 (n=61) | event renders: 6.29 / 6.29 / 6.29 (n=1) | handle_event: -

```
2026-10-26 07:41:31.465 editor-window[15054:43060] compose-rust: parity system_theme light
2026-10-26 07:41:31.466 editor-window[15054:43060] compose-rust: latency frames=363 average=24.98ms worst=88.91ms
2026-10-26 07:41:31.466 editor-window[15054:43060] compose-rust: parity frame_idle_avg_ms 24.98 frames=363 worst_ms=88.91
exit status 0
```

