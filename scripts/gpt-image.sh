#!/usr/bin/env bash
# Concept art via the OpenAI Images API.
#
#   scripts/gpt-image.sh [options] "prompt"
#
#   -o NAME     output base name in art/concepts/ (default: timestamp)
#   -s SIZE     1024x1024 | 1536x1024 | 1024x1536 | WxH (default 1024x1024)
#   -q QUALITY  low | medium | high | xhigh | max | auto (default medium)
#   -n COUNT    number of variants (default 1)
#   -r FILE     reference image; repeatable. Switches to the edits endpoint.
#   -t          transparent background (png)
#   -S          skip the shared style prefix from art/style.txt
#
# The contents of art/style.txt are prepended to every prompt, so the whole
# set stays in one art direction. Needs OPENAI_API_KEY (see .env.example).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out_dir="$root/art/concepts"
style_file="$root/art/style.txt"

gen_model="${GPT_IMAGE_MODEL:-gpt-image-2.5-flare}"
edit_model="${GPT_IMAGE_EDIT_MODEL:-gpt-image-2.5-sunburst}"

name="$(date +%Y%m%d-%H%M%S)"
size="1024x1024"
quality="medium"
count=1
background="auto"
use_style=1
refs=()

usage() { sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

while getopts "o:s:q:n:r:tSh" opt; do
  case "$opt" in
    o) name="$OPTARG" ;;
    s) size="$OPTARG" ;;
    q) quality="$OPTARG" ;;
    n) count="$OPTARG" ;;
    r) refs+=("$OPTARG") ;;
    t) background="transparent" ;;
    S) use_style=0 ;;
    h) usage 0 ;;
    *) usage 1 ;;
  esac
done
shift $((OPTIND - 1))
[ $# -ge 1 ] || usage 1
prompt="$*"

: "${OPENAI_API_KEY:?OPENAI_API_KEY is not set (copy .env.example to .env)}"

if [ "$use_style" = 1 ] && [ -s "$style_file" ]; then
  prompt="$(cat "$style_file")"$'\n\n'"$prompt"
fi

mkdir -p "$out_dir"
response="$(mktemp)"
trap 'rm -f "$response"' EXIT

if [ ${#refs[@]} -eq 0 ]; then
  model="$gen_model"
  jq -n --arg model "$model" --arg prompt "$prompt" --arg size "$size" \
        --arg quality "$quality" --arg background "$background" --argjson n "$count" \
        '{model: $model, prompt: $prompt, size: $size, quality: $quality,
          background: $background, n: $n, output_format: "png"}' |
    curl -sS https://api.openai.com/v1/images/generations \
      -H "Authorization: Bearer $OPENAI_API_KEY" \
      -H "Content-Type: application/json" \
      -d @- -o "$response"
else
  model="$edit_model"
  form=(-F "model=$model" -F "prompt=$prompt" -F "size=$size" -F "quality=$quality"
        -F "background=$background" -F "n=$count" -F "output_format=png")
  for ref in "${refs[@]}"; do
    [ -f "$ref" ] || { echo "reference not found: $ref" >&2; exit 1; }
    form+=(-F "image[]=@$ref")
  done
  curl -sS https://api.openai.com/v1/images/edits \
    -H "Authorization: Bearer $OPENAI_API_KEY" \
    "${form[@]}" -o "$response"
fi

if jq -e '.error' "$response" >/dev/null 2>&1; then
  jq -r '"OpenAI error: \(.error.message)"' "$response" >&2
  exit 1
fi

total="$(jq '.data | length' "$response")"
for i in $(seq 0 $((total - 1))); do
  if [ "$total" -eq 1 ]; then file="$out_dir/$name.png"; else file="$out_dir/$name-$((i + 1)).png"; fi
  jq -r ".data[$i].b64_json" "$response" | base64 -d > "$file"
  echo "$file"
done

# Keep the prompt next to the images, to reproduce or iterate later.
printf 'model: %s\nsize: %s\nquality: %s\nrefs: %s\n\n%s\n' \
  "$model" "$size" "$quality" "${refs[*]:-none}" "$prompt" > "$out_dir/$name.prompt.txt"
