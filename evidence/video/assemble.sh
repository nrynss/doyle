#!/bin/sh
# Build the story-order reel from the demo-shoot takes, outside the repo.
# Usage: sh assemble.sh [takes-dir] [out.mkv]
# Defaults: ~/work/demo-shoot/doyle/takes  ./doyle-reel.mkv (next to takes:
# pass an explicit out path outside the repository; takes never enter the repo).
set -eu
TAKES="${1:-$HOME/work/demo-shoot/doyle/takes}"
OUT="${2:-$TAKES/doyle-reel.mkv}"
LIST="$(mktemp)"
trap 'rm -f "$LIST"' EXIT
for t in \
  2026-10-31-without-doyle-migration.mkv \
  2026-11-01-t7.3-rerun-guard-precheck.mkv \
  2026-11-01-t7.3-rerun-denial.mkv \
  2026-11-01-t7.3-rerun-migration-1.mkv \
  2026-11-01-t7.3-rerun-review-open-desktop.mkv \
  2026-11-01-t7.3-rerun-review-settled-desktop.mkv \
  2026-11-01-t7.3-rerun-review-flag-1-corrected-desktop.mkv \
  2026-11-01-t7.3-rerun-corrected-memory.mkv \
  2026-11-01-t7.3-rerun-migration-2.mkv \
  2026-11-01-t7.3-rerun-final-counts.mkv ; do
  if [ ! -f "$TAKES/$t" ]; then echo "missing take: $t" >&2; exit 1; fi
  printf "file '%s/%s'\n" "$TAKES" "$t" >>"$LIST"
  echo "in: $t ($(ffprobe -v error -show_entries format=duration -of csv=p=0 "$TAKES/$t")s)"
done
if ffmpeg -v error -y -f concat -safe 0 -i "$LIST" -c copy "$OUT"; then
  echo "reel (stream copy): $OUT"
else
  echo "stream copy failed; re-encoding" >&2
  ffmpeg -v error -y -f concat -safe 0 -i "$LIST" -c:v libx264 -pix_fmt yuv420p -c:a aac "$OUT"
  echo "reel (re-encoded): $OUT"
fi
echo "total: $(ffprobe -v error -show_entries format=duration -of csv=p=0 "$OUT")s"
