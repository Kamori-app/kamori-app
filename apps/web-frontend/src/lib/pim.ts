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

export interface PimSnapshotBranchV2 {
  projection_resource_id: string;
  head_operation_id: string;
  deleted: boolean;
  materialized_projection: Uint8Array;
}

export interface PimSnapshotV2 {
  schema_version: number;
  covers_through_space_seq: number;
  resource_kind: PimResourceKind;
  resource_id: string;
  branches: PimSnapshotBranchV2[];
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
  /** Server transport sequence; zero means the operation is still local-only. */
  spaceSeq: number;
  logicalResourceId: string;
  projectionId: string;
  parentOperationId?: string;
  seedProjectionId?: string;
  kind: PimResourceKind;
  title: string;
  completed: boolean;
  fields: Record<string, PimValue>;
  deleted: boolean;
  /** Complete iCalendar/vCard projection, including properties unknown to Kamori. */
  materializedProjection: string;
}

export interface MaterializedPimState {
  version: 5;
  items: MaterializedPimItem[];
  operations: MaterializedOperationState[];
  cursors: Record<string, number>;
}

const padDatePart = (value: number, width = 2): string =>
  value.toString().padStart(width, "0");

/** Converts an instant to the canonical UTC value used in iCalendar fields. */
export const dateToIcalendarUtc = (value: Date): string => {
  if (!Number.isFinite(value.getTime())) {
    throw new Error("Date-time is invalid.");
  }
  return [
    padDatePart(value.getUTCFullYear(), 4),
    padDatePart(value.getUTCMonth() + 1),
    padDatePart(value.getUTCDate()),
    "T",
    padDatePart(value.getUTCHours()),
    padDatePart(value.getUTCMinutes()),
    padDatePart(value.getUTCSeconds()),
    "Z",
  ].join("");
};

/**
 * Parses an HTML datetime-local value as wall-clock time in the user's current
 * timezone and rejects DST-normalized/nonexistent values instead of silently
 * saving a different appointment time.
 */
export const localDateTimeToIcalendarUtc = (value: string): string => {
  const match =
    /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2}))?$/.exec(value);
  if (!match) {
    throw new Error("Date-time must use the browser's local date-time format.");
  }
  const [, year, month, day, hour, minute, second = "00"] = match;
  const parts = [year, month, day, hour, minute, second].map(Number);
  const instant = new Date(
    parts[0],
    parts[1] - 1,
    parts[2],
    parts[3],
    parts[4],
    parts[5],
    0,
  );
  const roundTrip = [
    instant.getFullYear(),
    instant.getMonth() + 1,
    instant.getDate(),
    instant.getHours(),
    instant.getMinutes(),
    instant.getSeconds(),
  ];
  if (parts.some((part, index) => part !== roundTrip[index])) {
    throw new Error("This local date-time does not exist in the current timezone.");
  }
  return dateToIcalendarUtc(instant);
};

export const encodePimOperation = (operation: PimOperationV1): Uint8Array => encode(operation);

const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const isResourceKind = (value: unknown): value is PimResourceKind =>
  value === "calendar_event" || value === "task" || value === "contact";

const isUuid = (value: unknown): value is string =>
  typeof value === "string" && UUID_PATTERN.test(value);

const isPimValue = (value: unknown): value is PimValue => {
  if (!isRecord(value) || typeof value.type !== "string") return false;
  switch (value.type) {
    case "text":
      return typeof value.value === "string";
    case "integer":
      return typeof value.value === "number" && Number.isSafeInteger(value.value);
    case "boolean":
      return typeof value.value === "boolean";
    case "text_list":
      return (
        Array.isArray(value.value) &&
        value.value.every((entry) => typeof entry === "string")
      );
    case "bytes":
      return value.value instanceof Uint8Array;
    case "null":
      return value.value === undefined || value.value === null;
    default:
      return false;
  }
};

const decodeDependencies = (value: unknown): value is string[] =>
  Array.isArray(value) &&
  // PIM v1 is intentionally single-parent. Multi-parent convergence is only
  // valid once the payload is upgraded to a specified CRDT codec.
  value.length <= 1 &&
  value.every(isUuid) &&
  new Set(value).size === value.length;

export const decodePimOperation = (bytes: Uint8Array): PimOperationV1 => {
  const value: unknown = decode(bytes);
  if (
    !isRecord(value) ||
    !isResourceKind(value.resource_kind) ||
    !isUuid(value.resource_id) ||
    !decodeDependencies(value.dependencies)
  ) {
    throw new Error("Encrypted PIM operation has an invalid envelope.");
  }
  if (value.operation === "upsert") {
    if (
      !isRecord(value.fields) ||
      !Object.values(value.fields).every(isPimValue) ||
      !(value.raw_projection instanceof Uint8Array)
    ) {
      throw new Error("Encrypted PIM upsert payload is invalid.");
    }
    return value as unknown as PimUpsertV1;
  }
  if (value.operation === "delete") {
    if (
      value.projection_resource_id !== undefined &&
      value.projection_resource_id !== null &&
      (typeof value.projection_resource_id !== "string" ||
        value.projection_resource_id.length === 0)
    ) {
      throw new Error("Encrypted PIM delete payload is invalid.");
    }
    return value as unknown as PimDeleteV1;
  }
  throw new Error("Encrypted PIM operation kind is unsupported.");
};

export const encodePimSnapshot = (snapshot: PimSnapshotV2): Uint8Array => encode(snapshot);

export const decodePimSnapshot = (bytes: Uint8Array): PimSnapshotV2 => {
  const value: unknown = decode(bytes);
  if (!isRecord(value) || value.schema_version !== 2) {
    throw new Error("Unsupported or invalid PIM snapshot version.");
  }
  if (
    typeof value.covers_through_space_seq !== "number" ||
    !Number.isSafeInteger(value.covers_through_space_seq) ||
    value.covers_through_space_seq < 0 ||
    !isResourceKind(value.resource_kind) ||
    !isUuid(value.resource_id) ||
    !Array.isArray(value.branches) ||
    value.branches.length === 0
  ) {
    throw new Error("Encrypted PIM snapshot payload is invalid.");
  }
  const projectionIds = new Set<string>();
  const headIds = new Set<string>();
  for (const branch of value.branches) {
    if (
      !isRecord(branch) ||
      typeof branch.projection_resource_id !== "string" ||
      branch.projection_resource_id.length === 0 ||
      !isUuid(branch.head_operation_id) ||
      typeof branch.deleted !== "boolean" ||
      !(branch.materialized_projection instanceof Uint8Array) ||
      projectionIds.has(branch.projection_resource_id) ||
      headIds.has(branch.head_operation_id)
    ) {
      throw new Error("Encrypted PIM snapshot branch is invalid.");
    }
    projectionIds.add(branch.projection_resource_id);
    headIds.add(branch.head_operation_id);
    if (!branch.deleted && branch.materialized_projection.length === 0) {
      throw new Error("A live PIM snapshot must contain a projection.");
    }
    if (!branch.deleted) {
      new TextDecoder("utf-8", { fatal: true }).decode(branch.materialized_projection);
    }
  }
  return value as unknown as PimSnapshotV2;
};

interface ProjectionLine {
  property: string;
  value: string;
  componentId: number | null;
}

const splitProjectionLine = (line: string): [string, string] | null => {
  let quoted = false;
  let escaped = false;
  for (let index = 0; index < line.length; index += 1) {
    const char = line[index];
    if (escaped) {
      escaped = false;
    } else if (char === "\\" && quoted) {
      escaped = true;
    } else if (char === '"') {
      quoted = !quoted;
    } else if (char === ":" && !quoted) {
      return [line.slice(0, index), line.slice(index + 1)];
    }
  }
  return null;
};

const projectionPropertyName = (head: string): string =>
  (head.split(";", 1)[0]?.split(".").at(-1) ?? "").toUpperCase();

const parseProjectionLines = (
  projection: string,
): { lines: ProjectionLine[]; components: string[] } => {
  const unfolded: string[] = [];
  for (const line of projection.replaceAll("\r\n", "\n").split("\n")) {
    if ((line.startsWith(" ") || line.startsWith("\t")) && unfolded.length > 0) {
      unfolded[unfolded.length - 1] += line.slice(1);
    } else if (line.length > 0) {
      unfolded.push(line);
    }
  }
  const components: string[] = [];
  const stack: number[] = [];
  const lines: ProjectionLine[] = [];
  for (const logical of unfolded) {
    const content = splitProjectionLine(logical);
    if (!content) continue;
    const [head, value] = content;
    const property = projectionPropertyName(head);
    const componentId = stack.at(-1) ?? null;
    if (property === "BEGIN") {
      components.push(value.trim().toUpperCase());
      stack.push(components.length - 1);
    } else if (property === "END") {
      stack.pop();
    }
    lines.push({ property, value, componentId });
  }
  return { lines, components };
};

export const unescapeProjectionText = (value: string): string => {
  let result = "";
  for (let index = 0; index < value.length; index += 1) {
    const char = value[index];
    if (char !== "\\" || index + 1 >= value.length) {
      result += char;
      continue;
    }
    const escaped = value[(index += 1)];
    result += escaped === "n" || escaped === "N" ? "\n" : escaped;
  }
  return result;
};

export const projectionProperty = (
  projection: string,
  kind: PimResourceKind,
  name: string,
): string | undefined => {
  const parsed = parseProjectionLines(projection);
  const componentName =
    kind === "contact" ? "VCARD" : kind === "task" ? "VTODO" : "VEVENT";
  const ids = parsed.components
    .map((component, id) => (component === componentName ? id : -1))
    .filter((id) => id >= 0);
  const primary =
    ids.find(
      (id) =>
        !parsed.lines.some(
          (line) => line.componentId === id && line.property === "RECURRENCE-ID",
        ),
    ) ?? ids[0];
  if (primary === undefined) return undefined;
  return parsed.lines.find(
    (line) =>
      line.componentId === primary && line.property === name.toUpperCase(),
  )?.value;
};

export const projectionFields = (
  projection: string,
  kind: PimResourceKind,
): Record<string, PimValue> => {
  const fields: Record<string, PimValue> = {};
  const title = projectionProperty(
    projection,
    kind,
    kind === "contact" ? "FN" : "SUMMARY",
  );
  if (title !== undefined) {
    fields.title = { type: "text", value: unescapeProjectionText(title) };
  }
  for (const [field, property] of [
    ["email", "EMAIL"],
    ["phone", "TEL"],
    ["starts_at", "DTSTART"],
    ["ends_at", "DTEND"],
  ] as const) {
    const value = projectionProperty(projection, kind, property);
    if (value !== undefined) {
      fields[field] = {
        type: "text",
        value:
          field === "email" || field === "phone"
            ? unescapeProjectionText(value)
            : value,
      };
    }
  }
  if (kind === "task") {
    fields.completed = {
      type: "boolean",
      value:
        projectionProperty(projection, kind, "STATUS")?.toUpperCase() ===
        "COMPLETED",
    };
  }
  return fields;
};
