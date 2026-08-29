import { describe, expect, test } from "bun:test";
import {
  buildRestorePimOperation,
  listDeletedPimItems,
} from "./pimHistory.ts";

const state = (overrides = {}) => ({
  spaceId: "space-a",
  clientOpId: "00000000-0000-4000-8000-000000000001",
  spaceSeq: 1,
  logicalResourceId: "10000000-0000-4000-8000-000000000001",
  projectionId: "10000000-0000-4000-8000-000000000001",
  kind: "contact",
  title: "Alice",
  completed: false,
  fields: { title: { type: "text", value: "Alice" } },
  deleted: false,
  materializedProjection:
    "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:alice\r\nFN:Alice\r\nX-PRIVATE:keep\r\nEND:VCARD\r\n",
  ...overrides,
});

describe("PIM trash history", () => {
  test("lists only a tombstone that is still a branch head", () => {
    const live = state();
    const deleted = state({
      clientOpId: "00000000-0000-4000-8000-000000000002",
      spaceSeq: 2,
      parentOperationId: live.clientOpId,
      deleted: true,
      materializedProjection: "",
    });

    const items = listDeletedPimItems([live, deleted]);

    expect(items).toHaveLength(1);
    expect(items[0].title).toBe("Alice");
    expect(items[0].tombstoneOperationId).toBe(deleted.clientOpId);
    expect(items[0].restorableProjection).toContain("X-PRIVATE:keep");
  });

  test("removes a tombstone from Trash after a restored child appears", () => {
    const live = state();
    const deleted = state({
      clientOpId: "00000000-0000-4000-8000-000000000002",
      parentOperationId: live.clientOpId,
      deleted: true,
      materializedProjection: "",
    });
    const restored = state({
      clientOpId: "00000000-0000-4000-8000-000000000003",
      parentOperationId: deleted.clientOpId,
    });

    expect(listDeletedPimItems([live, deleted, restored])).toEqual([]);
  });

  test("restores the lossless projection as a child of the tombstone", () => {
    const live = state();
    const deleted = state({
      clientOpId: "00000000-0000-4000-8000-000000000002",
      parentOperationId: live.clientOpId,
      deleted: true,
      materializedProjection: "",
    });
    const item = listDeletedPimItems([live, deleted])[0];

    const operation = buildRestorePimOperation(item);

    expect(operation.operation).toBe("upsert");
    expect(operation.dependencies).toEqual([deleted.clientOpId]);
    expect(new TextDecoder().decode(operation.raw_projection)).toContain(
      "X-PRIVATE:keep",
    );
  });

  test("does not pretend snapshot-only tombstones can be restored", () => {
    const deleted = state({
      deleted: true,
      title: "",
      fields: {},
      materializedProjection: "",
    });
    const item = listDeletedPimItems([deleted])[0];

    expect(item.restorableProjection).toBeNull();
    expect(() => buildRestorePimOperation(item)).toThrow(
      "not available on this device",
    );
  });
});
