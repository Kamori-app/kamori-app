import { decode, encode } from "@msgpack/msgpack";
import { RRule } from "rrule";

export type PimResourceKind = "calendar_event" | "task" | "contact";

export type PimValue =
  | { type: "text"; value: string }
  | { type: "integer"; value: number }
  | { type: "boolean"; value: boolean }
  | { type: "text_list"; value: string[] }
  | { type: "record"; value: Record<string, string> }
  | { type: "records"; value: Record<string, string>[] }
  | { type: "bytes"; value: Uint8Array }
  | { type: "null" };

export const CURRENT_PIM_SCHEMA_VERSION = 2;

export type PimTemporal =
  | { kind: "date"; date: string }
  | { kind: "utc"; utc: string }
  | { kind: "zoned_datetime"; local: string; timezone: string; utc: string };

export interface PimUpsertV1 {
  operation: "upsert";
  schema_version: number;
  resource_kind: PimResourceKind;
  resource_id: string;
  dependencies: string[];
  fields: Record<string, PimValue>;
  raw_projection: Uint8Array;
}

export interface PimDeleteV1 {
  operation: "delete";
  schema_version: number;
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

export const encodePimOperation = (operation: PimOperationV1): Uint8Array => {
  const bytes = encode(operation);
  decodePimOperation(bytes);
  return bytes;
};

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
    case "record":
      return (
        isRecord(value.value) &&
        Object.values(value.value).every((entry) => typeof entry === "string")
      );
    case "records":
      return (
        Array.isArray(value.value) &&
        value.value.every(
          (entry) =>
            isRecord(entry) &&
            Object.values(entry).every((part) => typeof part === "string"),
        )
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
  const schemaVersion = value.schema_version ?? 1;
  if (
    typeof schemaVersion !== "number" ||
    !Number.isSafeInteger(schemaVersion) ||
    schemaVersion < 1 ||
    schemaVersion > CURRENT_PIM_SCHEMA_VERSION
  ) {
    throw new Error("Encrypted PIM operation has an unsupported schema version.");
  }
  value.schema_version = schemaVersion;
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

export const encodePimSnapshot = (snapshot: PimSnapshotV2): Uint8Array => {
  const bytes = encode(snapshot);
  decodePimSnapshot(bytes);
  return bytes;
};

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
  head: string;
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
): { lines: ProjectionLine[]; components: string[]; parents: (number | null)[] } => {
  const unfolded: string[] = [];
  for (const line of projection.replaceAll("\r\n", "\n").split("\n")) {
    if ((line.startsWith(" ") || line.startsWith("\t")) && unfolded.length > 0) {
      unfolded[unfolded.length - 1] += line.slice(1);
    } else if (line.length > 0) {
      unfolded.push(line);
    }
  }
  const components: string[] = [];
  const parents: (number | null)[] = [];
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
      parents.push(componentId);
      stack.push(components.length - 1);
    } else if (property === "END") {
      stack.pop();
    }
    lines.push({ head, property, value, componentId });
  }
  return { lines, components, parents };
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

const projectionProperties = (
  projection: string,
  kind: PimResourceKind,
  name: string,
): ProjectionLine[] => {
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
  if (primary === undefined) return [];
  return parsed.lines.filter(
    (line) =>
      line.componentId === primary && line.property === name.toUpperCase(),
  );
};

const projectionParameter = (head: string, name: string): string | undefined => {
  const parameters: string[] = [];
  let start = 0;
  let quoted = false;
  let escaped = false;
  for (let index = 0; index < head.length; index += 1) {
    const character = head[index];
    if (escaped) {
      escaped = false;
    } else if (character === "\\" && quoted) {
      escaped = true;
    } else if (character === '"') {
      quoted = !quoted;
    } else if (character === ";" && !quoted) {
      parameters.push(head.slice(start, index));
      start = index + 1;
    }
  }
  parameters.push(head.slice(start));
  for (const parameter of parameters.slice(1)) {
    const separator = parameter.indexOf("=");
    if (separator < 0) continue;
    if (parameter.slice(0, separator).toUpperCase() !== name.toUpperCase()) continue;
    return parameter
      .slice(separator + 1)
      .replace(/^"|"$/g, "")
      .replaceAll('\\"', '"')
      .replaceAll("\\\\", "\\");
  }
  return undefined;
};

const splitEscaped = (value: string, separator: string): string[] => {
  const parts: string[] = [];
  let current = "";
  let escaped = false;
  for (const character of value) {
    if (escaped) {
      current += `\\${character}`;
      escaped = false;
    } else if (character === "\\") {
      escaped = true;
    } else if (character === separator) {
      parts.push(unescapeProjectionText(current));
      current = "";
    } else {
      current += character;
    }
  }
  if (escaped) current += "\\";
  parts.push(unescapeProjectionText(current));
  return parts;
};

const compactDateToIso = (value: string): string =>
  /^\d{8}$/.test(value)
    ? `${value.slice(0, 4)}-${value.slice(4, 6)}-${value.slice(6, 8)}`
    : value;

const temporalFromProjection = (line: ProjectionLine): PimValue => {
  if (projectionParameter(line.head, "VALUE")?.toUpperCase() === "DATE") {
    return {
      type: "record",
      value: { kind: "date", date: compactDateToIso(line.value) },
    };
  }
  const timezone = projectionParameter(line.head, "TZID");
  if (timezone) {
    return {
      type: "record",
      value: { kind: "zoned_datetime", local: line.value, timezone },
    };
  }
  return { type: "record", value: { kind: "utc", utc: line.value } };
};

const labeledProjectionRecords = (lines: ProjectionLine[]): Record<string, string>[] =>
  lines.map((line) => ({
    label:
      projectionParameter(line.head, "X-KAMORI-LABEL") ??
      projectionParameter(line.head, "TYPE")?.split(",")[0]?.toLowerCase() ??
      "",
    value: unescapeProjectionText(line.value),
    raw_head: line.head,
  }));

const managedAlarmMinutes = (
  projection: string,
  kind: PimResourceKind,
): number | undefined => {
  if (kind === "contact") return undefined;
  const parsed = parseProjectionLines(projection);
  const componentName = kind === "task" ? "VTODO" : "VEVENT";
  const componentIds = parsed.components
    .map((component, id) => (component === componentName ? id : -1))
    .filter((id) => id >= 0);
  const primary =
    componentIds.find(
      (id) =>
        !parsed.lines.some(
          (line) => line.componentId === id && line.property === "RECURRENCE-ID",
        ),
    ) ?? componentIds[0];
  if (primary === undefined) return undefined;
  const alarms = parsed.components
    .map((component, id) =>
      component === "VALARM" && parsed.parents[id] === primary ? id : -1,
    )
    .filter((id) => id >= 0);
  for (const alarm of alarms) {
    const managed = parsed.lines.some(
      (line) =>
        line.componentId === alarm &&
        line.property === "X-KAMORI-MANAGED" &&
        line.value.toUpperCase() === "TRUE",
    );
    if (!managed) continue;
    const trigger = parsed.lines.find(
      (line) => line.componentId === alarm && line.property === "TRIGGER",
    )?.value;
    const match = trigger && /^-PT(\d+)M$/i.exec(trigger);
    if (match) return Number(match[1]);
  }
  return undefined;
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
    ["starts_at", "DTSTART"],
    ["ends_at", "DTEND"],
    ["due_at", "DUE"],
  ] as const) {
    const line = projectionProperties(projection, kind, property)[0];
    if (line) fields[field] = temporalFromProjection(line);
  }
  if (kind === "task") {
    fields.completed = {
      type: "boolean",
      value:
        projectionProperty(projection, kind, "STATUS")?.toUpperCase() ===
        "COMPLETED",
    };
    const completedAt = projectionProperty(projection, kind, "COMPLETED");
    if (completedAt) fields.completed_at = { type: "text", value: completedAt };
    const priority = Number(projectionProperty(projection, kind, "PRIORITY"));
    if (Number.isSafeInteger(priority) && priority >= 0 && priority <= 9) {
      fields.priority = { type: "integer", value: priority };
    }
  }
  const textProperties =
    kind === "contact"
      ? ([
          ["organization", "ORG"],
          ["job_title", "TITLE"],
          ["birthday", "BDAY"],
          ["url", "URL"],
          ["notes", "NOTE"],
        ] as const)
      : ([
          ["location", "LOCATION"],
          ["notes", "DESCRIPTION"],
          ["recurrence_rule", "RRULE"],
        ] as const);
  for (const [field, property] of textProperties) {
    const value = projectionProperty(projection, kind, property);
    if (value !== undefined) {
      fields[field] = {
        type: "text",
        value: property === "RRULE" ? value : unescapeProjectionText(value),
      };
    }
  }
  const categories = projectionProperty(projection, kind, "CATEGORIES");
  if (categories !== undefined) {
    fields.categories = { type: "text_list", value: splitEscaped(categories, ",") };
  }
  const reminder = managedAlarmMinutes(projection, kind);
  if (reminder !== undefined) {
    fields.reminder_minutes = { type: "integer", value: reminder };
  }
  if (kind === "contact") {
    const emails = labeledProjectionRecords(projectionProperties(projection, kind, "EMAIL"));
    const phones = labeledProjectionRecords(projectionProperties(projection, kind, "TEL"));
    const addresses = projectionProperties(projection, kind, "ADR").map((line) => {
      const parts = splitEscaped(line.value, ";");
      return {
        label:
          projectionParameter(line.head, "X-KAMORI-LABEL") ??
          projectionParameter(line.head, "TYPE")?.split(",")[0]?.toLowerCase() ??
          "",
        raw_head: line.head,
        po_box: parts[0] ?? "",
        extended: parts[1] ?? "",
        street: parts[2] ?? "",
        locality: parts[3] ?? "",
        region: parts[4] ?? "",
        postal_code: parts[5] ?? "",
        country: parts[6] ?? "",
      };
    });
    if (emails.length > 0) {
      fields.emails = { type: "records", value: emails };
      fields.email = { type: "text", value: emails[0].value };
    }
    if (phones.length > 0) {
      fields.phones = { type: "records", value: phones };
      fields.phone = { type: "text", value: phones[0].value };
    }
    if (addresses.length > 0) fields.addresses = { type: "records", value: addresses };
    const structuredName = projectionProperty(projection, kind, "N");
    if (structuredName !== undefined) {
      const parts = splitEscaped(structuredName, ";");
      fields.name = {
        type: "record",
        value: {
          family: parts[0] ?? "",
          given: parts[1] ?? "",
          middle: parts[2] ?? "",
          prefix: parts[3] ?? "",
          suffix: parts[4] ?? "",
        },
      };
    }
    fields.favorite = {
      type: "boolean",
      value:
        projectionProperty(projection, kind, "X-KAMORI-FAVORITE")?.toUpperCase() ===
        "TRUE",
    };
  }
  return fields;
};

export const localDateTimeToTemporal = (
  value: string,
  timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC",
): PimTemporal => {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2}))?$/.exec(value);
  if (!match) throw new Error("Date-time must use the browser's local date-time format.");
  const local = `${match[1]}${match[2]}${match[3]}T${match[4]}${match[5]}${match[6] ?? "00"}`;
  const instant = zonedCompactLocalToDate(local, timezone);
  if (!instant) {
    throw new Error("This local date-time does not exist in the selected timezone.");
  }
  return {
    kind: "zoned_datetime",
    local,
    timezone,
    utc: dateToIcalendarUtc(instant),
  };
};

export const temporalToDate = (value: PimValue | undefined): Date | undefined => {
  if (value?.type === "text") {
    const match = /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z$/.exec(value.value);
    return match
      ? new Date(Date.UTC(+match[1], +match[2] - 1, +match[3], +match[4], +match[5], +match[6]))
      : undefined;
  }
  if (value?.type !== "record") return undefined;
  if (value.value.kind === "date") return new Date(`${value.value.date}T00:00:00`);
  if (
    value.value.kind === "zoned_datetime" &&
    value.value.local &&
    value.value.timezone &&
    !value.value.utc
  ) {
    return zonedCompactLocalToDate(value.value.local, value.value.timezone);
  }
  const utc = value.value.utc;
  if (!utc) return undefined;
  const match = /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z$/.exec(utc);
  return match
    ? new Date(Date.UTC(+match[1], +match[2] - 1, +match[3], +match[4], +match[5], +match[6]))
    : undefined;
};

const zonedCompactLocalToDate = (local: string, timezone: string): Date | undefined => {
  const match = /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})$/.exec(local);
  if (!match) return undefined;
  const wanted = match.slice(1).map(Number);
  const desiredTimestamp = Date.UTC(
    wanted[0],
    wanted[1] - 1,
    wanted[2],
    wanted[3],
    wanted[4],
    wanted[5],
  );
  let timestamp = desiredTimestamp;
  try {
    const formatter = new Intl.DateTimeFormat("en-US", {
      timeZone: timezone,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hourCycle: "h23",
    });
    for (let iteration = 0; iteration < 2; iteration += 1) {
      const parts = Object.fromEntries(
        formatter
          .formatToParts(new Date(timestamp))
          .filter((part) => part.type !== "literal")
          .map((part) => [part.type, Number(part.value)]),
      );
      const rendered = Date.UTC(
        parts.year,
        parts.month - 1,
        parts.day,
        parts.hour,
        parts.minute,
        parts.second,
      );
      timestamp += desiredTimestamp - rendered;
    }
    const result = new Date(timestamp);
    const roundTrip = Object.fromEntries(
      formatter
        .formatToParts(result)
        .filter((part) => part.type !== "literal")
        .map((part) => [part.type, Number(part.value)]),
    );
    return roundTrip.year === wanted[0] &&
      roundTrip.month === wanted[1] &&
      roundTrip.day === wanted[2] &&
      roundTrip.hour === wanted[3] &&
      roundTrip.minute === wanted[4] &&
      roundTrip.second === wanted[5]
      ? result
      : undefined;
  } catch {
    return undefined;
  }
};

export const temporalToInputValue = (value: PimValue | undefined): string => {
  if (value?.type === "record" && value.value.kind === "zoned_datetime") {
    const local = value.value.local;
    return /^\d{8}T\d{6}$/.test(local)
      ? `${local.slice(0, 4)}-${local.slice(4, 6)}-${local.slice(6, 8)}T${local.slice(9, 11)}:${local.slice(11, 13)}`
      : "";
  }
  const date = temporalToDate(value);
  if (!date) return "";
  return [
    padDatePart(date.getFullYear(), 4),
    "-",
    padDatePart(date.getMonth() + 1),
    "-",
    padDatePart(date.getDate()),
    "T",
    padDatePart(date.getHours()),
    ":",
    padDatePart(date.getMinutes()),
  ].join("");
};

/** Applies iCalendar's exclusive DTEND semantics to a local calendar day. */
export const intervalOccursOnDay = (
  startsAt: Date | undefined,
  endsAt: Date | undefined,
  day: Date,
): boolean => {
  if (!startsAt) return false;
  const effectiveEnd = endsAt ?? startsAt;
  const dayStart = new Date(day.getFullYear(), day.getMonth(), day.getDate());
  const dayEnd = new Date(day.getFullYear(), day.getMonth(), day.getDate() + 1);
  return startsAt < dayEnd && effectiveEnd > dayStart;
};

/** Expands an RFC 5545 RRULE and tests occurrence intervals against one local day. */
export const recurringIntervalOccursOnDay = (
  startsValue: PimValue | undefined,
  endsValue: PimValue | undefined,
  recurrenceRule: string,
  day: Date,
): boolean => {
  const startsAt = temporalToDate(startsValue);
  if (!startsAt) return false;
  const explicitEnd = temporalToDate(endsValue);
  if (!recurrenceRule) {
    const effectiveEnd = explicitEnd ?? (
      startsValue?.type === "record" && startsValue.value.kind === "date"
        ? new Date(startsAt.getFullYear(), startsAt.getMonth(), startsAt.getDate() + 1)
        : new Date(startsAt.getTime() + 1)
    );
    return intervalOccursOnDay(startsAt, effectiveEnd, day);
  }

  const dateOnly = startsValue?.type === "record" && startsValue.value.kind === "date";
  const dayStart = dateOnly
    ? new Date(Date.UTC(day.getFullYear(), day.getMonth(), day.getDate()))
    : new Date(day.getFullYear(), day.getMonth(), day.getDate());
  const dayEnd = dateOnly
    ? new Date(Date.UTC(day.getFullYear(), day.getMonth(), day.getDate() + 1))
    : new Date(day.getFullYear(), day.getMonth(), day.getDate() + 1);
  const ruleStart = dateOnly
    ? new Date(Date.UTC(startsAt.getFullYear(), startsAt.getMonth(), startsAt.getDate()))
    : startsAt;
  const ruleEnd = dateOnly && explicitEnd
    ? new Date(Date.UTC(explicitEnd.getFullYear(), explicitEnd.getMonth(), explicitEnd.getDate()))
    : explicitEnd;
  const duration = Math.max(1, (ruleEnd?.getTime() ?? (
    dateOnly ? ruleStart.getTime() + 86_400_000 : ruleStart.getTime() + 1
  )) - ruleStart.getTime());
  try {
    const timezone = startsValue?.type === "record" &&
        startsValue.value.kind === "zoned_datetime"
      ? startsValue.value.timezone
      : null;
    const options = RRule.parseString(recurrenceRule);
    options.dtstart = ruleStart;
    if (timezone) options.tzid = timezone;
    const recurrence = new RRule(options);
    return recurrence
      .between(new Date(dayStart.getTime() - duration), dayEnd, true)
      .some((occurrence) =>
        occurrence < dayEnd &&
        new Date(occurrence.getTime() + duration) > dayStart
      );
  } catch {
    const fallbackEnd = ruleEnd ?? new Date(ruleStart.getTime() + duration);
    return intervalOccursOnDay(ruleStart, fallbackEnd, dayStart);
  }
};
