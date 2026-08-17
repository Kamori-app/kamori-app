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

export interface PimSnapshotV1 {
  schema_version: number;
  covers_through_space_seq: number;
  resource_kind: PimResourceKind;
  resource_id: string;
  projection_resource_id: string;
  head_operation_id: string;
  deleted: boolean;
  materialized_projection: Uint8Array;
}

export interface MaterializedPimItem {
  spaceId: string;
  resourceId: string;
  kind: PimResourceKind;
  title: string;
  completed: boolean;
  fields: Record<string, PimValue>;
  headOperationId: string;
  conflict: boolean;
  projectionId?: string;
}

export interface MaterializedOperationState {
  spaceId: string;
  clientOpId: string;
  logicalResourceId: string;
  projectionId: string;
  kind: PimResourceKind;
  title: string;
  completed: boolean;
  fields: Record<string, PimValue>;
  deleted: boolean;
}

export interface MaterializedPimState {
  version: 2;
  items: MaterializedPimItem[];
  operations: MaterializedOperationState[];
}

export const encodePimOperation = (operation: PimOperationV1): Uint8Array => encode(operation);

export const decodePimOperation = (bytes: Uint8Array): PimOperationV1 =>
  decode(bytes) as PimOperationV1;

export const decodePimSnapshot = (bytes: Uint8Array): PimSnapshotV1 => {
  const snapshot = decode(bytes) as PimSnapshotV1;
  if (snapshot.schema_version !== 1) {
    throw new Error(`Unsupported PIM snapshot version ${snapshot.schema_version}.`);
  }
  return snapshot;
};
