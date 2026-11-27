"""S3-compatible object storage helper (MinIO / Backblaze B2) using boto3 (TDD 9.2)."""

from __future__ import annotations

from pathlib import Path
import boto3
from botocore.config import Config
from botocore.exceptions import ClientError

from pace_audio.config import AudioWorkerConfig


class AudioStorage:
    def __init__(self, cfg: AudioWorkerConfig) -> None:
        self._bucket = cfg.s3_bucket
        self._client = boto3.client(
            "s3",
            endpoint_url=cfg.s3_endpoint,
            region_name=cfg.s3_region,
            aws_access_key_id=cfg.s3_access_key,
            aws_secret_access_key=cfg.s3_secret_key,
            config=Config(s3={"addressing_style": "path"}),
        )

    def download_file(self, key: str, dest: Path) -> None:
        dest.parent.mkdir(parents=True, exist_ok=True)
        self._client.download_file(self._bucket, key, str(dest))

    def upload_file(self, src: Path, key: str, content_type: str) -> None:
        self._client.upload_file(
            str(src),
            self._bucket,
            key,
            ExtraArgs={"ContentType": content_type},
        )

    def head_size(self, key: str) -> int | None:
        try:
            resp = self._client.head_object(Bucket=self._bucket, Key=key)
            return int(resp["ContentLength"])
        except ClientError:
            return None

    def delete_object(self, key: str) -> None:
        try:
            self._client.delete_object(Bucket=self._bucket, Key=key)
        except ClientError:
            pass
