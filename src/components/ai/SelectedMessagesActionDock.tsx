import { AiSelectionActionBar } from "./AiSelectionActionBar";

interface SelectedMessagesActionDockProps {
  count: number;
  onClear: () => void;
  onCopy: () => void;
  onExport: () => void;
  onInsert?: () => void;
}

export function SelectedMessagesActionDock({
  count,
  onClear,
  onCopy,
  onExport,
  onInsert,
}: SelectedMessagesActionDockProps) {
  if (count <= 0) return null;

  return (
    <div className="flex justify-center px-3 py-1.5">
      <AiSelectionActionBar
        count={count}
        onInsert={onInsert}
        onCopy={onCopy}
        onExport={onExport}
        onClear={onClear}
      />
    </div>
  );
}
