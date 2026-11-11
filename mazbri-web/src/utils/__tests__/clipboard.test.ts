import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { copyTextToClipboard } from "../clipboard";

interface FakeEnv {
  secure: boolean;
  clipboard?: { writeText: (v: string) => Promise<void> };
  execResult?: boolean | "throw";
}

const g = globalThis as Record<string, unknown>;
const saved = {
  window: g.window,
  navigator: Object.getOwnPropertyDescriptor(globalThis, "navigator"),
  document: g.document,
};

function install(env: FakeEnv) {
  const log = { clipboard: [] as string[], exec: [] as string[], attached: 0 };
  let current: { value: string } | null = null;
  g.window = { isSecureContext: env.secure };
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: env.clipboard
      ? {
          clipboard: {
            writeText: async (v: string) => {
              log.clipboard.push(v);
              await env.clipboard!.writeText(v);
            },
          },
        }
      : {},
  });
  g.document = {
    body: {
      appendChild: (el: { value: string }) => {
        log.attached += 1;
        current = el;
      },
      removeChild: () => {
        log.attached -= 1;
        current = null;
      },
    },
    createElement: () => ({
      value: "",
      style: {},
      setAttribute: () => undefined,
      focus: () => undefined,
      select: () => undefined,
    }),
    execCommand: (cmd: string) => {
      if (env.execResult === "throw") throw new Error("not allowed");
      log.exec.push(`${cmd}:${current?.value ?? ""}`);
      return env.execResult ?? true;
    },
  };
  return log;
}

afterEach(() => {
  g.window = saved.window;
  g.document = saved.document;
  if (saved.navigator) {
    Object.defineProperty(globalThis, "navigator", saved.navigator);
  }
});

test("安全上下文且有 navigator.clipboard 时走剪贴板 API", async () => {
  const log = install({
    secure: true,
    clipboard: { writeText: async () => {} },
  });
  assert.equal(await copyTextToClipboard(" abc-123 "), true);
  assert.deepEqual(log.clipboard, ["abc-123"]);
  assert.deepEqual(log.exec, []);
});

test("http 非安全上下文没有 clipboard 时降级 execCommand，并复制完整值", async () => {
  const log = install({ secure: false });
  const id = "80f515be-f998-40cb-b488-0c663402ef3b";
  assert.equal(await copyTextToClipboard(id), true);
  assert.deepEqual(log.exec, [`copy:${id}`]);
  assert.equal(log.attached, 0, "临时 textarea 必须移除");
});

test("clipboard 写入被拒时降级 execCommand", async () => {
  const log = install({
    secure: true,
    clipboard: {
      writeText: async () => {
        throw new Error("denied");
      },
    },
  });
  assert.equal(await copyTextToClipboard("x"), true);
  assert.deepEqual(log.exec, ["copy:x"]);
});

test("clipboard 与 execCommand 都失败时返回 false", async () => {
  install({ secure: false, execResult: false });
  assert.equal(await copyTextToClipboard("x"), false);
  const log = install({ secure: false, execResult: "throw" });
  assert.equal(await copyTextToClipboard("x"), false);
  assert.equal(log.attached, 0);
});

test("空值不复制", async () => {
  const log = install({ secure: false });
  assert.equal(await copyTextToClipboard("   "), false);
  assert.deepEqual(log.exec, []);
});
