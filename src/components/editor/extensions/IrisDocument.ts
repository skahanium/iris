import { Node } from "@tiptap/core";

/** Iris note document: mandatory title block followed by body blocks. */
export const IrisDocument = Node.create({
  name: "doc",
  topNode: true,
  content: "noteTitle block+",
});
