import { decode, encode } from "@msgpack/msgpack";

export type PimResourceKind = "calendar_event" | "task" | "contact";

export type PimValue =
  | { type: "text"; value: string }
  | { type: "integer"; value: number }
  | { type: "boolean"; value: boolean }
  | { type: "text_list"; value: string[] }
  | { type: "bytes"; value: Uint8Array }
  | { type: "null" };

export interface PimUpsertV1 {
  operation: "upsert";
  resource_kind: PimResourceKind;
  resource_id: string;
  dependencies: string[];
  fields: Record<string, PimValue>;
  raw_projection: Uint8Array;
}

export interface PimDeleteV1 {
  operation: "delete";
  resource_kind: PimResourceKind;
  resource_id: string;
  dependencies: string[];
  projection_resource_id?: string | null;
}

export type PimOperationV1 = PimUpsertV1 | PimDeleteV1;

export interface MaterializedPimItem {
  spaceId: string;
  resourceId: string;
  kind: PimResourceKind;
  title: string;
  completed: boolean;
  fields: Record<string, PimValue>;
  headOperationId: string;
  conflict: boolean;
}

export const encodePimOperation = (operation: PimOperationV1): Uint8Array => encode(operation);

export const decodePimOperation = (bytes: Uint8Array): PimOperationV1 =>
  decode(bytes) as PimOperationV1;
