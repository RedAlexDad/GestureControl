#!/usr/bin/env bash
#
# Загружает файлы MediaPipe, которые не хранятся в git: библиотеку C API
# libmediapipe.so и модель hand_landmarker.task. Кладёт их в models/.
#
#   scripts/fetch-models.sh
#
# Нужны curl и unzip. Версия библиотеки совпадает с MediaPipe 0.10.35.

set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
models="$root/models"
mkdir -p "$models"

task="$models/hand_landmarker.task"
if [ ! -f "$task" ]; then
  echo "качаю hand_landmarker.task"
  curl -fL \
    "https://storage.googleapis.com/mediapipe-models/hand_landmarker/hand_landmarker/float16/1/hand_landmarker.task" \
    -o "$task"
fi

lib="$models/libmediapipe.so"
if [ ! -f "$lib" ]; then
  version="0.10.35"
  wheel="https://files.pythonhosted.org/packages/32/8f/1bc57dbc9b7b03c8f875aac23380ec57e9002cc02fe6720045fb263f3966/mediapipe-${version}-py3-none-manylinux_2_28_x86_64.whl"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  echo "качаю libmediapipe.so из колеса mediapipe"
  curl -fL "$wheel" -o "$tmp/mediapipe.whl"
  unzip -o -q "$tmp/mediapipe.whl" "mediapipe/tasks/c/libmediapipe.so" -d "$tmp"
  cp "$tmp/mediapipe/tasks/c/libmediapipe.so" "$lib"
fi

echo "готово:"
echo "  $task"
echo "  $lib"
