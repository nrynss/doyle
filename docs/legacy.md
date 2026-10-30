# Legacy COBOL runtime

How GnuCOBOL runs on this machine, and how the T9.1 sample is built and run.
The gate never needs COBOL: `tools/check.sh` is rustfmt, clippy and the
tests, and no test or build step invokes `cobc`. The compiler runs only by
hand (or in an `#[ignore]`d live test); `testdata/legacy/runtime/` holds the
recorded program, input, output and trace.

## Install

GnuCOBOL is not in the main Arch/CachyOS repositories. What is there is
`gcc-gcobol` (the GCC COBOL frontend, 16.2.1), which is a different compiler
and is not used here.

Two routes work, both giving GnuCOBOL 3.2.0:

1. **AUR package `gnucobol 3.2-8`** (tried first, as planned). The PKGBUILD
   builds upstream `gnucobol-3.2.tar.xz` from `ftp.gnu.org` with `--with-db
   --with-json=json-c --with-xml2`. A system install needs root (`paru -S
   gnucobol` asks for a sudo password). Without root the same source builds
   into a user prefix:
   ```
   ./configure --prefix=$HOME/.local/gnucobol-3.2 \
     --without-db --with-json=json-c --with-xml2
   make -j$(nproc) && make install
   ```
   `--without-db` is the one local deviation: the Berkeley DB headers the
   AUR build wants are not installed here. Result: `cobc (GnuCOBOL) 3.2.0`
   built against GCC 16.2.1, at `$HOME/.local/gnucobol-3.2/bin/cobc`.
   Needs `LD_LIBRARY_PATH=$HOME/.local/gnucobol-3.2/lib` at run time
   (or `ldconfig`, which needs root).

2. **Container** (no root needed beyond the `docker` group):
   ```
   docker run --rm -v "$PWD:/w" -w /w debian:stable-slim bash -c "
     apt-get update -qq &&
     DEBIAN_FRONTEND=noninteractive apt-get install -y -qq gnucobol &&
     cobc --version"
   ```
   Tested on `debian:stable-slim` digest
   `sha256:13e0a2e78aa0554ad5ff66e4e260c6dbcdff807eb1f39d43b9fb0288aae88b04`.
   The `gnucobol` metapackage (APT version 5) pulls `gnucobol3 3.2-3` and
   reports `cobc (GnuCOBOL) 3.2.0` built against C 14.2.0.

Both compilers build and run the sample identically.

## The sample

In `testdata/legacy/runtime/`:

| File | What |
|---|---|
| `BILLBATCH.cbl` | batch program, free format (`>>SOURCE FORMAT IS FREE`) |
| `patient.cpy` | copybook: `PATIENT-RECORD`, 34 chars fixed-width |
| `patients.dat` | 5 fixed-width input records (6 + 20 + 7 + 1) |
| `report.txt` | recorded output of the run below |
| `trace.txt` | recorded paragraph trace of the same run |

The program `OPEN`s the input `LINE SEQUENTIAL` through the copybook,
`PERFORM`s `CALC-LATE-FEE` per record (`COMPUTE WS-LATE-FEE ROUNDED =
BALANCE * 0.015`, the 1.5% rule) or `LOG-EXEMPT` when `EXEMPT-FLAG = "Y"`,
writes one detail line per record through `WRITE-DETAIL`, then totals through
`WRITE-TOTALS`. The input exercises `ROUNDED` both ways: 33.33 rounds
0.49995 up to 0.50, 999.99 rounds 14.99985 up to 15.00, the exempt 250.00
carries 0.00. Totals: 5 records, balance 1,393.33, fee 17.15.

Reproduce, from `testdata/legacy/runtime/` (filenames are `ASSIGN`ed
literally, so run in that directory):

```
cobc -x -ftrace -o /tmp/billbatch BILLBATCH.cbl
COB_SET_TRACE=1 /tmp/billbatch 2> trace.txt
```

`cobc` prints one `_FORTIFY_SOURCE` warning from the system headers on the
local build; the compile still succeeds. The run writes `report.txt`,
takes no stdin, and exits 0 with empty stdout. The trace goes to stderr.

## The trace

Paragraph tracing needs both halves: compile with `-ftrace` (paragraph
scope) or `-ftraceall` (paragraph plus every statement: `OPEN`, `READ`,
`PERFORM`, `UNTIL`, ...), and run with `COB_SET_TRACE=1` (any of `1`/`Y`
works). Without the variable the binary runs silently even when compiled
with `-ftrace`.

Each executed paragraph emits one stderr line naming the program, the
paragraph and its source line:

```
Program-Id:  BILLBATCH        Paragraph: CALC-LATE-FEE  Line:  66
```

The third input record is exempt, so the recorded `trace.txt` shows
`LOG-EXEMPT` (line 69) where the other four records show `CALC-LATE-FEE`
(line 66) — the trace tells which paragraphs ran for this input. That
per-input paragraph list is what the differential run (T9.8) will read to
blame mismatched outputs. A `DISPLAY`-per-paragraph convention would give
the same information but mixes trace with program output; `-ftrace` keeps
it on stderr with line numbers, so it is the convention.
