#!/usr/bin/env bash
set -euo pipefail
# All credentials/endpoints are synthetic and supplied by the isolated runner.
source /exercise/blob.env
export RCLONE_CONFIG_PRIMARY_TYPE=s3 RCLONE_CONFIG_PRIMARY_PROVIDER=Other
export RCLONE_CONFIG_PRIMARY_ACCESS_KEY_ID=$PRIMARY_S3_KEY_ID
export RCLONE_CONFIG_PRIMARY_SECRET_ACCESS_KEY=$PRIMARY_S3_APPLICATION_KEY
export RCLONE_CONFIG_PRIMARY_ENDPOINT=$PRIMARY_S3_ENDPOINT
export RCLONE_CONFIG_PRIMARY_REGION=$PRIMARY_S3_REGION
export RCLONE_CONFIG_DR_TYPE=s3 RCLONE_CONFIG_DR_PROVIDER=Other
export RCLONE_CONFIG_DR_ACCESS_KEY_ID=$DR_S3_ACCESS_KEY
export RCLONE_CONFIG_DR_SECRET_ACCESS_KEY=$DR_S3_SECRET_KEY
export RCLONE_CONFIG_DR_ENDPOINT=$DR_S3_ENDPOINT
export RCLONE_CONFIG_DR_REGION=$DR_S3_REGION
export KAMORI_BACKUP_ENV=/exercise/blob.env
case "$1" in
  setup)
    mc alias set primary "$PRIMARY_S3_ENDPOINT" "$PRIMARY_S3_KEY_ID" "$PRIMARY_S3_APPLICATION_KEY"
    mc alias set dr "$DR_S3_ENDPOINT" "$DR_ROOT_USER" "$DR_ROOT_PASSWORD"
    mc mb primary/ciphertext dr/ciphertext
    mc admin user add dr "$DR_S3_ACCESS_KEY" "$DR_S3_SECRET_KEY"
    mc admin policy create dr put-only /exercise/put-only.json
    mc admin policy attach dr put-only --user "$DR_S3_ACCESS_KEY"
    for file in /exercise/objects/*; do rclone copyto "$file" "primary:ciphertext/$(basename "$file")"; done
    ;;
  policy-green)
    mc admin policy detach dr put-only --user "$DR_S3_ACCESS_KEY"
    mc admin policy create dr replicate-verify /exercise/replicate-verify.json
    mc admin policy attach dr replicate-verify --user "$DR_S3_ACCESS_KEY"
    ;;
  replicate) bash /exercise/replicate-blobs ;;
  read)
    mkdir -p /exercise/restored
    while read -r hash size key; do
      rclone cat "dr:ciphertext/$key" > "/exercise/restored/$key"
      [[ $(wc -c < "/exercise/restored/$key") == "$size" ]] || { echo "size mismatch: $key" >&2; exit 1; }
      [[ $(sha256sum "/exercise/restored/$key" | cut -d' ' -f1) == "$hash" ]] || { echo "SHA-256 mismatch: $key" >&2; exit 1; }
      echo "verified $key bytes=$size sha256=$hash"
    done < /exercise/manifest.txt
    ;;
  missing) mc rm dr/ciphertext/ciphertext-1.bin ;;
  repair) mc cp /exercise/objects/ciphertext-1.bin dr/ciphertext/ciphertext-1.bin ;;
  corrupt)
    cp /exercise/objects/ciphertext-1.bin /exercise/corrupt.bin
    printf '\377' | dd of=/exercise/corrupt.bin bs=1 seek=0 conv=notrunc status=none
    mc cp /exercise/corrupt.bin dr/ciphertext/ciphertext-1.bin
    ;;
  *) exit 2 ;;
esac
