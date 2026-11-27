import { useRef } from "react";
import { Upload } from "lucide-react";
import {
  Button,
  Modal,
  SegmentedControl,
  Tabs,
  Textarea,
} from "@/components/ui";
import { useT } from "@/locales";

export type WorkbenchImportTab = "curl" | "postman" | "openapi";

export interface WorkbenchImportModalProps {
  open: boolean;
  onClose: () => void;
  importTab: WorkbenchImportTab;
  onImportTabChange: (next: WorkbenchImportTab) => void;
  importCurlText: string;
  onImportCurlTextChange: (next: string) => void;
  importJsonText: string;
  onImportJsonTextChange: (next: string) => void;
  mode?: "merge" | "replace";
  onModeChange?: (next: "merge" | "replace") => void;
  showMode?: boolean;
  importing: boolean;
  onConfirm: () => void | Promise<void>;
}

export function WorkbenchImportModal({
  open,
  onClose,
  importTab,
  onImportTabChange,
  importCurlText,
  onImportCurlTextChange,
  importJsonText,
  onImportJsonTextChange,
  mode = "replace",
  onModeChange,
  showMode = false,
  importing,
  onConfirm,
}: WorkbenchImportModalProps) {
  const { t } = useT();
  const fileRef = useRef<HTMLInputElement | null>(null);

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const text = String(reader.result || "");
      if (importTab === "curl") onImportCurlTextChange(text);
      else onImportJsonTextChange(text);
    };
    reader.readAsText(file);
    e.target.value = "";
  };

  return (
    <Modal
      open={open}
      onClose={importing ? () => {} : onClose}
      title={t("upstreamConnector.import")}
      size="md"
      closeOnBackdrop={false}
      testId="workbench-import-modal"
      footer={
        <>
          <Button
            type="button"
            variant="outline"
            disabled={importing}
            onClick={onClose}
          >
            {t("common.cancel")}
          </Button>
          <Button
            type="button"
            variant="primary"
            loading={importing}
            onClick={() => void onConfirm()}
          >
            {t("common.confirm")}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        {showMode && onModeChange && (
          <SegmentedControl
            value={mode}
            onChange={onModeChange}
            items={[
              { key: "merge", label: t("upstreamConnector.importMerge") },
              { key: "replace", label: t("upstreamConnector.importReplace") },
            ]}
          />
        )}

        <Tabs<WorkbenchImportTab>
          value={importTab}
          onChange={onImportTabChange}
          items={[
            { key: "curl", label: "cURL" },
            { key: "postman", label: "Postman" },
            { key: "openapi", label: "OpenAPI" },
          ]}
        />

        {importTab === "curl" ? (
          <Textarea
            value={importCurlText}
            onChange={(e) => onImportCurlTextChange(e.target.value)}
            rows={10}
            aria-label="cURL"
            placeholder="curl -X GET 'https://api.example.com/posts' ..."
            className="font-mono-tech"
          />
        ) : (
          <Textarea
            value={importJsonText}
            onChange={(e) => onImportJsonTextChange(e.target.value)}
            rows={10}
            aria-label={
              importTab === "postman" ? "Postman JSON" : "OpenAPI JSON"
            }
            placeholder={
              importTab === "postman"
                ? "Postman Collection v2.1 JSON"
                : "OpenAPI 3 JSON"
            }
            className="font-mono-tech"
          />
        )}

        <div>
          <input
            ref={fileRef}
            type="file"
            accept=".json,.txt,.curl"
            aria-label={t("upstreamConnector.importFile")}
            className="hidden"
            onChange={handleFileChange}
          />
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => fileRef.current?.click()}
          >
            <Upload className="h-3.5 w-3.5" />
            {t("upstreamConnector.importFile")}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
