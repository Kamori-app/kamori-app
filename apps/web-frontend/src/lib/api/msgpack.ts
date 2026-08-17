import { decode, encode } from "@msgpack/msgpack";

const MSGPACK_CONTENT_TYPE = "application/msgpack";

export interface MsgpackRequestOptions {
  headers?: Record<string, string>;
  credentials?: RequestCredentials;
}

export class HttpStatusError extends Error {
  constructor(
    message: string,
    public readonly status: number,
  ) {
    super(message);
    this.name = "HttpStatusError";
  }
}

/**
 * Extracts the best available error message from failed HTTP responses.
 *
 * Supports JSON and MessagePack error payloads and falls back to plain text.
 */
const decodeErrorPayload = async (response: Response): Promise<string> => {
  const contentType = response.headers.get("content-type") ?? "";

  if (contentType.includes("application/json")) {
    try {
      const payload = (await response.json()) as { message?: string };
      return payload.message ?? `Request failed with status ${response.status}`;
    } catch {
      return `Request failed with status ${response.status}`;
    }
  }

  if (contentType.includes(MSGPACK_CONTENT_TYPE)) {
    try {
      const payload = decode(new Uint8Array(await response.arrayBuffer())) as {
        message?: string;
      };
      return payload.message ?? `Request failed with status ${response.status}`;
    } catch {
      return `Request failed with status ${response.status}`;
    }
  }

  try {
    const text = await response.text();
    return text || `Request failed with status ${response.status}`;
  } catch {
    return `Request failed with status ${response.status}`;
  }
};

/**
 * Sends MessagePack `POST` request and decodes MessagePack response.
 */
export const postMsgpack = async <TRequest, TResponse>(
  baseUrl: string,
  path: string,
  payload: TRequest,
  accessToken?: string | null,
  options?: MsgpackRequestOptions,
): Promise<TResponse> => {
  const headers: Record<string, string> = {
    "Content-Type": MSGPACK_CONTENT_TYPE,
    Accept: MSGPACK_CONTENT_TYPE,
    ...(accessToken ? { Authorization: `Bearer ${accessToken}` } : {}),
    ...(options?.headers ?? {}),
  };
  const response = await fetch(`${baseUrl.replace(/\/$/, "")}${path}`, {
    method: "POST",
    headers,
    ...(options?.credentials ? { credentials: options.credentials } : {}),
    body: encode(payload),
  });

  if (!response.ok) {
    throw new HttpStatusError(
      await decodeErrorPayload(response),
      response.status,
    );
  }

  const buffer = await response.arrayBuffer();
  return decode(new Uint8Array(buffer)) as TResponse;
};

/**
 * Sends MessagePack-aware `GET` request and decodes MessagePack response.
 */
export const getMsgpack = async <TResponse>(
  baseUrl: string,
  path: string,
  accessToken?: string | null,
  options?: MsgpackRequestOptions,
): Promise<TResponse> => {
  const headers: Record<string, string> = {
    Accept: MSGPACK_CONTENT_TYPE,
    ...(accessToken ? { Authorization: `Bearer ${accessToken}` } : {}),
    ...(options?.headers ?? {}),
  };
  const response = await fetch(`${baseUrl.replace(/\/$/, "")}${path}`, {
    method: "GET",
    headers,
    ...(options?.credentials ? { credentials: options.credentials } : {}),
  });

  if (!response.ok) {
    throw new HttpStatusError(
      await decodeErrorPayload(response),
      response.status,
    );
  }

  const buffer = await response.arrayBuffer();
  return decode(new Uint8Array(buffer)) as TResponse;
};

/** Sends an authenticated `DELETE` and decodes its MessagePack response. */
export const deleteMsgpack = async <TResponse>(
  baseUrl: string,
  path: string,
  accessToken?: string | null,
  options?: MsgpackRequestOptions,
): Promise<TResponse> => {
  const headers: Record<string, string> = {
    Accept: MSGPACK_CONTENT_TYPE,
    ...(accessToken ? { Authorization: `Bearer ${accessToken}` } : {}),
    ...(options?.headers ?? {}),
  };
  const response = await fetch(`${baseUrl.replace(/\/$/, "")}${path}`, {
    method: "DELETE",
    headers,
    ...(options?.credentials ? { credentials: options.credentials } : {}),
  });
  if (!response.ok) {
    throw new HttpStatusError(
      await decodeErrorPayload(response),
      response.status,
    );
  }
  return decode(new Uint8Array(await response.arrayBuffer())) as TResponse;
};
