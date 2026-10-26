## Window runs (MINGW64_NT-10.0-26100)

### idle-top

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| idle | generated (100000 lines) | 5.64 / 6.01 / 6.69 (n=120) | 15.85 / 22.83 / 32.40 (n=120) | 10.20 / 17.41 / 26.36 (n=120) | 16.00 / 16.00 / 16.00 (n=120) | - | - |

    editor-spike window: idle | generated (100000 lines) | scroll top | frames served 182 | rebuild 15.93 ms
    settling frames: interval 15.84 / 26.61 / 202.71 (n=61) | event renders: 8.07 / 8.07 / 8.07 (n=1) | handle_event: -

```
editor-spike window: ending the process after 50s
exit status 0
```

### page-top

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| page | generated (100000 lines) | 41.36 / 68.28 / 76.41 (n=100) | 74.24 / 105.16 / 140.21 (n=100) | 32.87 / 41.51 / 63.80 (n=100) | 16.00 / 16.00 / 16.00 (n=100) | - | - |

    editor-spike window: page | generated (100000 lines) | scroll top | frames served 162 | rebuild 12.88 ms
    settling frames: interval 15.85 / 27.20 / 97.37 (n=61) | event renders: 7.95 / 7.95 / 7.95 (n=1) | handle_event: -

```
editor-spike window: ending the process after 50s
exit status 0
```

### realtype-top

The probe did not report: see realtype-top.log.

```
editor-spike event: render 7.955 ms
editor-spike event: handle_event 0.083 ms (node 319, handler 2)
editor-spike event: render 10.814 ms
editor-spike event: handle_event 0.033 ms (node 319, handler 2)
editor-spike event: render 10.341 ms
editor-spike event: handle_event 0.031 ms (node 319, handler 2)
editor-spike event: render 10.017 ms
editor-spike event: handle_event 0.032 ms (node 319, handler 2)
editor-spike event: render 10.233 ms
editor-spike event: handle_event 0.034 ms (node 319, handler 2)
editor-spike event: render 10.179 ms
editor-spike window: ending the process after 32s
exit status 0
```

### type-real

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| type | stylist.rs (4465 lines) | 7.82 / 9.83 / 17.50 (n=1000) | 15.87 / 23.94 / 44.56 (n=1000) | 8.36 / 14.87 / 35.66 (n=1000) | 16.00 / 16.00 / 16.00 (n=1000) | 8.94 / 10.21 / 17.50 (n=500) | 16.07 / 24.62 / 44.56 (n=500) |

    editor-spike window: type | stylist.rs (4465 lines) | scroll top | frames served 1062 | rebuild 13.82 ms
    settling frames: interval 15.88 / 31.15 / 104.76 (n=61) | event renders: 7.61 / 7.61 / 7.61 (n=1) | handle_event: -

```
editor-spike window: ending the process after 110s
exit status 0
```

### type-top

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| type | generated (100000 lines) | 10.62 / 17.31 / 21.86 (n=1000) | 16.10 / 25.89 / 48.64 (n=1000) | 5.60 / 10.30 / 37.99 (n=1000) | 16.00 / 16.00 / 16.00 (n=1000) | 11.74 / 18.38 / 21.86 (n=500) | 17.70 / 28.17 / 48.64 (n=500) |

    editor-spike window: type | generated (100000 lines) | scroll top | frames served 1062 | rebuild 28.21 ms
    settling frames: interval 15.86 / 21.42 / 111.53 (n=61) | event renders: 8.09 / 8.09 / 8.09 (n=1) | handle_event: -

```
editor-spike window: ending the process after 110s
exit status 0
```

### type-transform

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| type | generated (100000 lines) | 10.26 / 14.16 / 19.89 (n=1000) | 16.04 / 22.80 / 45.17 (n=1000) | 5.81 / 11.73 / 30.22 (n=1000) | 16.00 / 16.00 / 16.00 (n=1000) | 11.37 / 14.94 / 19.89 (n=500) | 16.88 / 23.51 / 45.17 (n=500) |

    editor-spike window: type | generated (100000 lines) | scroll transform | frames served 1062 | rebuild 16.30 ms
    settling frames: interval 15.93 / 31.07 / 127.19 (n=61) | event renders: 10.42 / 10.42 / 10.42 (n=1) | handle_event: -

```
editor-spike window: ending the process after 110s
exit status 0
```

### wheel-real

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| wheel | stylist.rs (4465 lines) | 10.23 / 12.36 / 25.58 (n=300) | 33.62 / 40.60 / 58.19 (n=300) | 23.03 / 29.60 / 41.38 (n=300) | 16.00 / 16.00 / 16.00 (n=300) | - | - |

    editor-spike window: wheel | stylist.rs (4465 lines) | scroll top | frames served 362 | rebuild 14.47 ms
    settling frames: interval 15.92 / 26.64 / 108.74 (n=61) | event renders: 8.11 / 8.11 / 8.11 (n=1) | handle_event: -

```
editor-spike window: ending the process after 60s
exit status 0
```

### wheel-top

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| wheel | generated (100000 lines) | 12.59 / 19.11 / 23.20 (n=300) | 36.61 / 51.83 / 65.12 (n=300) | 23.82 / 33.16 / 43.51 (n=300) | 16.00 / 16.00 / 16.00 (n=300) | - | - |

    editor-spike window: wheel | generated (100000 lines) | scroll top | frames served 362 | rebuild 14.26 ms
    settling frames: interval 15.87 / 23.62 / 103.50 (n=61) | event renders: 8.26 / 8.26 / 8.26 (n=1) | handle_event: -

```
editor-spike window: ending the process after 60s
exit status 0
```

### wheel-transform

| scenario | file | host render | frame interval | interval minus host | renderer frame clock | key: host | key to next frame |
|---|---|---|---|---|---|---|---|
| wheel | generated (100000 lines) | 11.62 / 15.59 / 23.38 (n=300) | 34.74 / 43.74 / 64.15 (n=300) | 22.93 / 29.88 / 46.40 (n=300) | 16.00 / 16.00 / 16.00 (n=300) | - | - |

    editor-spike window: wheel | generated (100000 lines) | scroll transform | frames served 362 | rebuild 12.89 ms
    settling frames: interval 15.86 / 22.20 / 101.33 (n=61) | event renders: 8.10 / 8.10 / 8.10 (n=1) | handle_event: -

```
editor-spike window: ending the process after 60s
exit status 0
```

