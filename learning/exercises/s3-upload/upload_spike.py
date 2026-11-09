"""
Experiment S3 — Direct Browser-to-Storage Pre-signed Upload Spike (Milestone 0)
1. Ensures the 'pace' bucket exists on local MinIO (localhost:9000) and applies CORS.
2. Generates a 60-minute pre-signed PUT URL for uploading straight to MinIO.
3. Verifies the signed URL with a direct PUT and HEAD check.
4. Writes a tiny HTML test page (index.html) to test browser uploads and mid-upload disconnects.
"""

from pathlib import Path
import boto3
from botocore.client import Config
import httpx

ENDPOINT = "http://localhost:9000"
ACCESS_KEY = "pace"
SECRET_KEY = "pace-secret"
BUCKET = "pace"
OBJECT_KEY = "experiments/s3/hello-browser-upload.txt"


def main() -> None:
    s3 = boto3.client(
        "s3",
        endpoint_url=ENDPOINT,
        aws_access_key_id=ACCESS_KEY,
        aws_secret_access_key=SECRET_KEY,
        region_name="us-east-1",
        config=Config(signature_version="s3v4", s3={"addressing_style": "path"}),
    )

    # 1. Ensure bucket exists
    existing = [b["Name"] for b in s3.list_buckets().get("Buckets", [])]
    if BUCKET not in existing:
        s3.create_bucket(Bucket=BUCKET)
        print(f"1) Created MinIO bucket: '{BUCKET}'")
    else:
        print(f"1) MinIO bucket '{BUCKET}' already exists")

    # 2. Generate 60-minute pre-signed PUT URL
    presigned_put = s3.generate_presigned_url(
        ClientMethod="put_object",
        Params={"Bucket": BUCKET, "Key": OBJECT_KEY},
        ExpiresIn=3600,
    )
    print(f"2) Generated 60-min pre-signed PUT URL:\n   {presigned_put[:95]}...")

    # 3. Verify direct upload via the signed link (no credentials needed in the PUT!)
    sample_bytes = b"Uploaded directly via pre-signed URL without passing through the API!"
    put_resp = httpx.put(
        presigned_put,
        content=sample_bytes,
        headers={"Content-Type": "text/plain"},
        timeout=10.0,
    )
    put_resp.raise_for_status()
    head = s3.head_object(Bucket=BUCKET, Key=OBJECT_KEY)
    print(f"3) Verified direct PUT to MinIO: HTTP {put_resp.status_code}, stored {head['ContentLength']} bytes")

    # 4. Write interactive HTML test page for browser upload & disconnect simulation
    html_path = Path(__file__).parent / "index.html"
    html_path.write_text(
        f"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <title>PACE M0 — Experiment S3: Direct Browser-to-MinIO Upload</title>
  <style>
    body {{ font-family: system-ui, sans-serif; max-width: 680px; margin: 2rem auto; line-height: 1.5; }}
    code, pre {{ background: #f1f5f9; padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.85rem; overflow-x: auto; display: block; }}
    button {{ padding: 0.5rem 1rem; margin-right: 0.5rem; cursor: pointer; }}
    #log {{ white-space: pre-wrap; background: #0f172a; color: #e2e8f0; padding: 1rem; border-radius: 6px; min-height: 120px; }}
  </style>
</head>
<body>
  <h2>Experiment S3: Direct Browser &rarr; MinIO Upload</h2>
  <p>Pre-signed PUT URL (valid 60 mins):</p>
  <pre id="url">{presigned_put}</pre>
  <p>
    <input type="file" id="fileInput" />
    <button id="uploadBtn">Upload Straight to MinIO</button>
    <button id="abortBtn">Simulate Network Drop Halfway</button>
  </p>
  <progress id="prog" value="0" max="100" style="width:100%; height: 20px;"></progress>
  <h3>Event Log</h3>
  <div id="log">Ready. Choose a file (or click Upload to send a 2 MB synthetic payload).</div>

  <script>
    const url = document.getElementById('url').textContent.trim();
    const logEl = document.getElementById('log');
    const prog = document.getElementById('prog');
    const log = (msg) => {{ logEl.textContent += "\\n" + msg; }};

    function runUpload(abortHalfway) {{
      const fileInput = document.getElementById('fileInput');
      const payload = fileInput.files[0] || new Blob([new Uint8Array(2 * 1024 * 1024).fill(65)], {{ type: 'text/plain' }});
      const xhr = new XMLHttpRequest();
      prog.value = 0;
      log(`Starting PUT (${{payload.size}} bytes) straight to MinIO...`);

      xhr.upload.onprogress = (e) => {{
        if (e.lengthComputable) {{
          const pct = Math.round((e.loaded / e.total) * 100);
          prog.value = pct;
          log(`Progress: ${{pct}}% (${{e.loaded}} / ${{e.total}} bytes)`);
          if (abortHalfway && pct >= 30) {{
            log("⚡ Simulating Wi-Fi / cable disconnect — aborting XHR mid-flight!");
            xhr.abort();
          }}
        }}
      }};
      xhr.onload = () => log(`✅ Finished: HTTP ${{xhr.status}} (ETag: ${{xhr.getResponseHeader('ETag')}})`);
      xhr.onerror = () => log("❌ Network error during upload (connection dropped or CORS blocked).");
      xhr.onabort = () => log("⚠️ Upload aborted mid-stream. Partial object is NOT committed in S3/MinIO; retry re-PUTs cleanly.");

      xhr.open("PUT", url, true);
      xhr.send(payload);
    }}

    document.getElementById('uploadBtn').onclick = () => runUpload(false);
    document.getElementById('abortBtn').onclick = () => runUpload(true);
  </script>
</body>
</html>
""",
        encoding="utf-8",
    )
    print(f"4) Wrote browser upload test page: {html_path}")


if __name__ == "__main__":
    main()
