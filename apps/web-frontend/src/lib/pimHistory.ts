import {
  CURRENT_PIM_SCHEMA_VERSION,
  type MaterializedOperationState,
  type PimOperationV1,
  type PimResourceKind,
  type PimValue,
} from "./pim";

export interface DeletedPimItem {
  spaceId: string;
  resourceId: string;
  projectionId: string;
  kind: PimResourceKind;
  title: string;
  fields: Record<string, PimValue>;
  tombstoneOperationId: string;
  restorableProjection: string | null;
}

const operationKey = (spaceId: string, operationId: string): string =>
  `${spaceId}:${operationId}`;

/**
 * Returns only deleted branch heads. A tombstone followed by a restore or a
 * later edit is historical data, not an item that still belongs in Trash.
 */
export function listDeletedPimItems(
  operations: MaterializedOperationState[],
): DeletedPimItem[] {
  const operationsById = new Map(
    operations.map((operation) => [
      operationKey(operation.spaceId, operation.clientOpId),
      operation,
    ]),
  );
  const operationsWithChildren = new Set(
    operations.flatMap((operation) =>
      operation.parentOperationId
        ? [operationKey(operation.spaceId, operation.parentOperationId)]
        : [],
    ),
  );

  return operations
    .map((operation, index) => ({ operation, index }))
    .filter(
      ({ operation }) =>
        operation.deleted &&
        !operationsWithChildren.has(
          operationKey(operation.spaceId, operation.clientOpId),
        ),
    )
    .map(({ operation, index }) => {
      let ancestor = operation.parentOperationId
        ? operationsById.get(
            operationKey(operation.spaceId, operation.parentOperationId),
          )
        : undefined;
      const visited = new Set<string>();
      while (ancestor?.deleted && ancestor.parentOperationId) {
        const key = operationKey(ancestor.spaceId, ancestor.clientOpId);
        if (visited.has(key)) {
          ancestor = undefined;
          break;
        }
        visited.add(key);
        ancestor = operationsById.get(
          operationKey(ancestor.spaceId, ancestor.parentOperationId),
        );
      }
      const restorableAncestor =
        ancestor &&
        ancestor.spaceId === operation.spaceId &&
        ancestor.logicalResourceId === operation.logicalResourceId &&
        ancestor.kind === operation.kind &&
        !ancestor.deleted &&
        ancestor.materializedProjection.length > 0
          ? ancestor
          : undefined;
      return {
        item: {
          spaceId: operation.spaceId,
          resourceId: operation.logicalResourceId,
          projectionId: operation.projectionId,
          kind: operation.kind,
          title:
            operation.title.trim() ||
            restorableAncestor?.title.trim() ||
            "",
          fields:
            Object.keys(operation.fields).length > 0
              ? operation.fields
              : (restorableAncestor?.fields ?? {}),
          tombstoneOperationId: operation.clientOpId,
          restorableProjection:
            restorableAncestor?.materializedProjection ?? null,
        } satisfies DeletedPimItem,
        index,
      };
    })
    .sort((left, right) => right.index - left.index)
    .map(({ item }) => item);
}

/** Restores content as an auditable child of the current tombstone. */
export function buildRestorePimOperation(
  item: DeletedPimItem,
): PimOperationV1 {
  if (!item.restorableProjection) {
    throw new Error("The deleted content is not available on this device.");
  }
  return {
    operation: "upsert",
    schema_version: CURRENT_PIM_SCHEMA_VERSION,
    resource_kind: item.kind,
    resource_id: item.resourceId,
    dependencies: [item.tombstoneOperationId],
    fields: {},
    raw_projection: new TextEncoder().encode(item.restorableProjection),
  };
}
