import React, { useEffect, useRef } from "react";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { json } from "@codemirror/lang-json";
import { xml } from "@codemirror/lang-xml";
import {
  bracketMatching,
  foldGutter,
  foldKeymap,
  HighlightStyle,
  StreamLanguage,
  syntaxHighlighting,
} from "@codemirror/language";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import { yaml } from "@codemirror/legacy-modes/mode/yaml";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import {
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
  placeholder as cmPlaceholder,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { cn } from "./cn";

export type CodeLanguage = "json" | "xml" | "yaml" | "toml" | "text";

export interface CodeEditorProps {
  value: string;
  onChange?: (next: string) => void;
  language?: CodeLanguage;
  readOnly?: boolean;
  placeholder?: string;
  minHeight?: string | number;
  maxHeight?: string | number;
  className?: string;
  testId?: string;
}

function getLanguageExtension(language: CodeLanguage): Extension {
  switch (language) {
    case "json":
      return json();
    case "xml":
      return xml();
    case "yaml":
      return StreamLanguage.define(yaml);
    case "toml":
      return StreamLanguage.define(toml);
    default:
      return [];
  }
}

const lightHighlight = HighlightStyle.define([
  { tag: tags.keyword, color: "#4338ca", fontWeight: "600" },
  { tag: [tags.propertyName, tags.attributeName], color: "#0369a1" },
  { tag: [tags.string, tags.special(tags.string)], color: "#047857" },
  { tag: [tags.number, tags.bool, tags.null], color: "#b45309" },
  { tag: tags.comment, color: "#64748b", fontStyle: "italic" },
  { tag: [tags.tagName, tags.typeName], color: "#1d4ed8" },
  { tag: tags.punctuation, color: "#475569" },
]);

const darkHighlight = HighlightStyle.define([
  { tag: tags.keyword, color: "#93b4ff", fontWeight: "600" },
  { tag: [tags.propertyName, tags.attributeName], color: "#7dd3fc" },
  { tag: [tags.string, tags.special(tags.string)], color: "#6ee7b7" },
  { tag: [tags.number, tags.bool, tags.null], color: "#fcd34d" },
  { tag: tags.comment, color: "#94a3b8", fontStyle: "italic" },
  { tag: [tags.tagName, tags.typeName], color: "#a5b4fc" },
  { tag: tags.punctuation, color: "#cbd5e1" },
]);

function isDarkDocument(): boolean {
  if (typeof document === "undefined") return false;
  return document.documentElement.classList.contains("dark");
}

function toCssDimension(v?: string | number): string | undefined {
  if (v === undefined) return undefined;
  return typeof v === "number" ? `${v}px` : v;
}

function createEditorTheme(
  minHeight?: string | number,
  maxHeight?: string | number,
): Extension {
  const minH = toCssDimension(minHeight) ?? "180px";
  const maxH = toCssDimension(maxHeight);

  return EditorView.theme({
    "&": {
      backgroundColor: "var(--card)",
      color: "var(--foreground)",
      fontSize: "12px",
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
    },
    ".cm-scroller": {
      minHeight: minH,
      ...(maxH ? { maxHeight: maxH } : {}),
      overflow: "auto",
      lineHeight: "1.55",
    },
    ".cm-content": {
      padding: "8px 0",
      caretColor: "var(--primary)",
    },
    ".cm-line": {
      padding: "0 10px",
    },
    ".cm-gutters": {
      backgroundColor: "var(--muted)",
      color: "var(--muted-foreground)",
      borderRight: "1px solid var(--border)",
      minHeight: minH,
    },
    ".cm-activeLineGutter": {
      backgroundColor: "var(--hover-bg)",
      color: "var(--foreground)",
    },
    ".cm-activeLine": {
      backgroundColor: "color-mix(in srgb, var(--primary) 7%, transparent)",
    },
    ".cm-selectionBackground, &.cm-focused .cm-selectionBackground, ::selection":
      {
        backgroundColor: "var(--primary-soft)",
      },
    ".cm-cursor, .cm-dropCursor": {
      borderLeftColor: "var(--primary)",
    },
    ".cm-placeholder": {
      color: "var(--muted-foreground)",
    },
    "&.cm-focused": {
      outline: "none",
    },
  });
}

export function CodeEditor({
  value,
  onChange,
  language = "text",
  readOnly = false,
  placeholder,
  minHeight = 180,
  maxHeight,
  className,
  testId,
}: CodeEditorProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  const langCompartmentRef = useRef(new Compartment());
  const readOnlyCompartmentRef = useRef(new Compartment());
  const themeCompartmentRef = useRef(new Compartment());
  const highlightCompartmentRef = useRef(new Compartment());
  const placeholderCompartmentRef = useRef(new Compartment());

  const initialConfigRef = useRef({
    value,
    language,
    readOnly,
    minHeight,
    maxHeight,
    placeholder,
  });

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const init = initialConfigRef.current;

    const state = EditorState.create({
      doc: init.value,
      extensions: [
        lineNumbers(),
        foldGutter(),
        highlightActiveLine(),
        highlightActiveLineGutter(),
        bracketMatching(),
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap, ...foldKeymap, indentWithTab]),
        langCompartmentRef.current.of(getLanguageExtension(init.language)),
        readOnlyCompartmentRef.current.of([
          EditorState.readOnly.of(init.readOnly),
          EditorView.editable.of(!init.readOnly),
        ]),
        themeCompartmentRef.current.of(
          createEditorTheme(init.minHeight, init.maxHeight),
        ),
        highlightCompartmentRef.current.of(
          syntaxHighlighting(isDarkDocument() ? darkHighlight : lightHighlight),
        ),
        placeholderCompartmentRef.current.of(
          init.placeholder ? cmPlaceholder(init.placeholder) : [],
        ),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onChangeRef.current?.(update.state.doc.toString());
          }
        }),
      ],
    });

    const view = new EditorView({ state, parent: host });
    viewRef.current = view;

    const observer = new MutationObserver(() => {
      view.dispatch({
        effects: highlightCompartmentRef.current.reconfigure(
          syntaxHighlighting(isDarkDocument() ? darkHighlight : lightHighlight),
        ),
      });
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });

    return () => {
      observer.disconnect();
      view.destroy();
      viewRef.current = null;
    };
  }, []);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const current = view.state.doc.toString();
    if (value !== current) {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: value },
      });
    }
  }, [value]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: langCompartmentRef.current.reconfigure(
        getLanguageExtension(language),
      ),
    });
  }, [language]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: readOnlyCompartmentRef.current.reconfigure([
        EditorState.readOnly.of(readOnly),
        EditorView.editable.of(!readOnly),
      ]),
    });
  }, [readOnly]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: themeCompartmentRef.current.reconfigure(
        createEditorTheme(minHeight, maxHeight),
      ),
    });
  }, [minHeight, maxHeight]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: placeholderCompartmentRef.current.reconfigure(
        placeholder ? cmPlaceholder(placeholder) : [],
      ),
    });
  }, [placeholder]);

  return (
    <div
      ref={hostRef}
      data-testid={testId}
      className={cn(
        "w-full overflow-hidden rounded-md border border-input bg-card transition-colors hover:border-border-hover focus-within:border-primary focus-within:ring-2 focus-within:ring-primary/30",
        readOnly && "opacity-90",
        className,
      )}
    />
  );
}
