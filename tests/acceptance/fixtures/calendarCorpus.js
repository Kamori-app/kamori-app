import { mount } from "svelte";
import CalendarWorkspace from "../../../apps/web-frontend/src/lib/components/app/CalendarWorkspace.svelte";
import { projectionFields } from "../../../apps/web-frontend/src/lib/pim.ts";

window.openCalendar = (sources) => mount(CalendarWorkspace, {
  target: document.body,
  props: {
    spaces: [{ id: "s", name: "Synthetic calendar" }], selectedSpaceId: "s", canWrite: true,
    items: sources.map((source, index) => {
      const fields = projectionFields(source, "calendar_event");
      return { kind: "calendar_event", spaceId: "s", resourceId: `series-${index}`,
        projectionId: `series-${index}.ics`, headOperationId: `head-${index}`,
        title: fields.title.value, completed: false, conflict: false, fields };
    }),
    onSave: async (item, fields) => { window.saved = { item, fields }; },
    onDelete: async item => { window.deleted = item; },
  },
});
