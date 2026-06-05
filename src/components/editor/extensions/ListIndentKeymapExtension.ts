import { Extension } from "@tiptap/core";

export const ListIndentKeymapExtension = Extension.create({
  name: "listIndentKeymap",

  addKeyboardShortcuts() {
    return {
      Tab: () => {
        if (this.editor.isActive("taskList")) {
          return false;
        }
        return this.editor.commands.sinkListItem("listItem");
      },
      "Shift-Tab": () => {
        if (this.editor.isActive("taskList")) {
          return false;
        }
        return this.editor.commands.liftListItem("listItem");
      },
    };
  },
});
