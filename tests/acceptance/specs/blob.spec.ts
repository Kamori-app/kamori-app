import { createHash } from "node:crypto";

import { decode, encode } from "@msgpack/msgpack";
import { expect, test } from "@playwright/test";

import {
  captureAccessToken,
  createCollection,
  signupAndSignIn,
  uniqueAccount,
} from "./helpers";

interface UploadResponse {
  blob_id: string;
  stored: boolean;
}

const sha256 = (data: Uint8Array): Uint8Array =>
  new Uint8Array(createHash("sha256").update(data).digest());

test("@full encrypted blob integrity, idempotency, download, and storage quota", async ({ page }) => {
  const apiUrl = process.env.KAMORI_ACCEPTANCE_API_URL ?? "http://127.0.0.1:18080";
  const getAccessToken = captureAccessToken(page);
  await signupAndSignIn(page, uniqueAccount("blob"));
  const spaceId = await createCollection(page, "Blob acceptance space");
  const accessToken = getAccessToken();

  const data = new Uint8Array(1024 * 1024);
  crypto.getRandomValues(data.subarray(0, 65_536));
  const blobId = crypto.randomUUID();
  const uploadPayload = {
    blob_id: blobId,
    ciphertext_sha256: sha256(data),
    size_padded: data.length,
    data,
  };
  const upload = await fetch(`${apiUrl}/spaces/${spaceId}/blobs`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${accessToken}`,
      "Content-Type": "application/msgpack",
      Accept: "application/msgpack",
    },
    body: encode(uploadPayload),
  });
  expect(upload.status).toBe(200);
  expect(decode(new Uint8Array(await upload.arrayBuffer())) as UploadResponse).toEqual({
    blob_id: blobId,
    stored: true,
  });

  const duplicate = await fetch(`${apiUrl}/spaces/${spaceId}/blobs`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${accessToken}`,
      "Content-Type": "application/msgpack",
      Accept: "application/msgpack",
    },
    body: encode(uploadPayload),
  });
  expect(duplicate.status).toBe(200);
  expect(decode(new Uint8Array(await duplicate.arrayBuffer())) as UploadResponse).toEqual({
    blob_id: blobId,
    stored: false,
  });

  // Blob ids are scoped to a security space. Reusing an opaque client id in an
  // unrelated space must neither leak the first blob nor create a global-id
  // collision between tenants.
  const secondSpaceId = await createCollection(page, "Second blob namespace");
  const secondData = new Uint8Array(data);
  secondData[0] ^= 0xff;
  const secondSpaceUpload = await fetch(`${apiUrl}/spaces/${secondSpaceId}/blobs`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${accessToken}`,
      "Content-Type": "application/msgpack",
      Accept: "application/msgpack",
    },
    body: encode({
      blob_id: blobId,
      ciphertext_sha256: sha256(secondData),
      size_padded: secondData.length,
      data: secondData,
    }),
  });
  expect(secondSpaceUpload.status).toBe(200);
  expect(
    decode(new Uint8Array(await secondSpaceUpload.arrayBuffer())) as UploadResponse,
  ).toEqual({ blob_id: blobId, stored: true });

  const download = await fetch(`${apiUrl}/spaces/${spaceId}/blobs/${blobId}`, {
    headers: { Authorization: `Bearer ${accessToken}` },
  });
  expect(download.status).toBe(200);
  expect(download.headers.get("x-kamori-ciphertext-sha256")).toBe(
    Buffer.from(sha256(data)).toString("hex"),
  );
  expect(new Uint8Array(await download.arrayBuffer())).toEqual(data);

  const invalidHash = await fetch(`${apiUrl}/spaces/${spaceId}/blobs`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${accessToken}`,
      "Content-Type": "application/msgpack",
      Accept: "application/msgpack",
    },
    body: encode({
      ...uploadPayload,
      blob_id: crypto.randomUUID(),
      ciphertext_sha256: new Uint8Array(32),
    }),
  });
  expect(invalidHash.status).toBe(400);

  const oversizedForRemainingQuota = new Uint8Array(2 * 1024 * 1024);
  const quota = await fetch(`${apiUrl}/spaces/${spaceId}/blobs`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${accessToken}`,
      "Content-Type": "application/msgpack",
      Accept: "application/msgpack",
    },
    body: encode({
      blob_id: crypto.randomUUID(),
      ciphertext_sha256: sha256(oversizedForRemainingQuota),
      size_padded: oversizedForRemainingQuota.length,
      data: oversizedForRemainingQuota,
    }),
  });
  expect(quota.ok).toBe(false);
  const error = quota.headers.get("content-type")?.includes("application/msgpack")
    ? (decode(new Uint8Array(await quota.arrayBuffer())) as { message?: string })
    : ((await quota.json()) as { message?: string });
  expect(error.message).toContain("quota");
});
