import { mount } from "svelte";
import TaskWorkspace from "../../../apps/web-frontend/src/lib/components/app/TaskWorkspace.svelte";

window.openTask = (due) => mount(TaskWorkspace, {
  target: document.body,
  props: {
    spaces: [{ id: "s", name: "Personal" }], selectedSpaceId: "s", canWrite: true,
    items: [{ kind: "task", spaceId: "s", resourceId: "r", title: "Original", completed: false, fields: { due_at: due } }],
    onSave: async (_item, fields) => { window.saved = fields; },
    onToggle: async () => {}, onDelete: async () => {},
  },
});
