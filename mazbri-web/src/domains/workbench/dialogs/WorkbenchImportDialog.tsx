import React, { useRef } from "react";
import { useNavigate } from "react-router";
import { Upload } from "lucide-react";
import { useT } from "@/locales";
import {
  Button,
  CodeEditor,
  Modal,
  SegmentedControl,
  Tabs,
  Textarea,
  type TabOption,
} from "@/components/ui";
import {
  useWorkbenchStore,
  type ImportMode,
  type ImportTabId,
} from "../store";

export interface WorkbenchImportDialogProps {
  showMode?: boolean;
}

function WorkbenchImportDialogBody({ showMode = true }: WorkbenchImportDialogProps) {
  const { t } = useT();
  const navigate = useNavigate();
  const fileRef = useRef<HTMLInputElement | null>(null);

  const importing = useWorkbenchStore((s) => s.importing);
  const importTab = useWorkbenchStore((s) => s.importTab);
  const importMode = useWorkbenchStore((s) => s.importMode);
  const importCurlText = useWorkbenchStore((s) => s.importCurlText);
  const importJsonText = useWorkbenchStore((s) => s.importJsonText);
  const setShowImport = useWorkbenchStore((s) => s.setShowImport);
  const setImportTab = useWorkbenchStore((s) => s.setImportTab);
  const setImportMode = useWorkbenchStore((s) => s.setImportMode);
  const setImportCurlText = useWorkbenchStore((s) => s.setImportCurlText);
  const setImportJsonText = useWorkbenchStore((s) => s.setImportJsonText);
  const doImport = useWorkbenchStore((s) => s.doImport);

  const modeItems: readonly TabOption<ImportMode>[] = [
    { key: "merge", label: t("upstreamConnector.importMerge") },
    { key: "replace", label: t("upstreamConnector.importReplace") },
  ];

  const formatItems: readonly TabOption<ImportTabId>[] = [
    { key: "curl", label: "cURL" },
    { key: "postman", label: "Postman" },
    { key: "openapi", label: "OpenAPI 3.x" },
  ];

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const text = String(reader.result || "");
      if (importTab === "curl") {
        setImportCurlText(text);
      } else {
        setImportJsonText(text);
      }
    };
    reader.readAsText(file);
  };

  const canSubmit =
    importTab === "curl"
      ? Boolean(importCurlText.trim())
      : Boolean(importJsonText.trim());

  return (
    <Modal
      open
      onClose={() => {
        if (!importing) setShowImport(false);
      }}
      title={t("upstreamConnector.import")}
      size="lg"
      closeOnBackdrop={!importing}
      footer={
        <>
          <Button
            type="button"
            variant="outline"
            disabled={importing}
            onClick={() => setShowImport(false)}
          >
            {t("common.cancel")}
          </Button>
          <Button
            type="button"
            variant="primary"
            loading={importing}
            disabled={!canSubmit}
            onClick={() => void doImport(navigate)}
          >
            {t("common.confirm")}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex flex-wrap items-center gap-2">
            {showMode && (
              <SegmentedControl
                items={modeItems}
                value={importMode}
                onChange={setImportMode}
              />
            )}
            <div>
              <input
                ref={fileRef}
                type="file"
                accept=".json,.yaml,.yml,.txt,.curl"
                aria-label={t("upstreamConnector.importFile")}
                hidden
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

          <Tabs
            variant="inline"
            className="ml-auto"
            items={formatItems}
            value={importTab}
            onChange={setImportTab}
          />
        </div>

        {importTab === "curl" ? (
          <Textarea
            value={importCurlText}
            onChange={(e) => setImportCurlText(e.target.value)}
            rows={10}
            aria-label="cURL"
            spellCheck={false}
            placeholder="curl -X GET 'https://api.example.com/posts' -H 'Accept: application/json'"
            className="font-mono-tech leading-relaxed"
          />
        ) : (
          <CodeEditor
            value={importJsonText}
            onChange={setImportJsonText}
            language="json"
            minHeight={220}
            maxHeight={340}
            placeholder={
              importTab === "postman"
                ? '{"info": {"name": "Collection"}, "item": []}'
                : '{"openapi": "3.0.0", "paths": {}}'
            }
          />
        )}
      </div>
    </Modal>
  );
}

export function WorkbenchImportDialog(props: WorkbenchImportDialogProps) {
  const open = useWorkbenchStore((s) => s.showImport);
  if (!open) return null;
  return <WorkbenchImportDialogBody {...props} />;
}
