<template>
  <div class="json-path-helper">
    <ElPopover
      v-model:visible="visible"
      :disabled="disabled"
      placement="bottom"
      :width="400"
      trigger="click"
      popper-class="json-path-popper"
      @show="resetHelper"
    >
      <template #reference>
        <span
          :class="[
            plain ? 'helper-trigger-plain' : 'helper-trigger',
            { 'is-disabled': disabled }
          ]"
        >
          <ArtSvgIcon icon="ri:compass-discover-line" class="mr-0.5" />
          路径解析助手
        </span>
      </template>

      <div class="helper-content">
        <h4 class="helper-title">JSONPath 提取路径生成器</h4>

        <div v-if="step === 'input'" class="step-input">
          <p class="helper-intro-text">
            贴入上游 API 返回的**模拟 JSON 响应**数据，点击解析生成结构树：
          </p>
          <ElInput
            v-model="rawJsonInput"
            type="textarea"
            :rows="6"
            placeholder='例如:
{
  "code": 200,
  "data": {
    "token": "secret_token_123",
    "expires_in": 7200
  }
}'
            size="small"
            class="mb-3"
          />
          <div class="helper-actions">
            <ElButton
              type="primary"
              size="small"
              :disabled="!rawJsonInput.trim()"
              @click="parseJson"
            >
              开始解析 JSON
            </ElButton>
          </div>
        </div>

        <div v-else class="step-tree">
          <p class="helper-intro-text">
            点击展开树节点，**直接点击**要提取的字段即可自动生成 JSONPath：
          </p>

          <div class="tree-container">
            <ElTree
              :data="treeData"
              node-key="jsonPath"
              :expand-on-click-node="true"
              size="small"
              @node-click="handleNodeClick"
            >
              <template #default="{ data }">
                <span class="custom-tree-node">
                  <span class="node-label">{{ data.name }}</span>
                  <span v-if="data.valuePreview" class="node-preview">
                    : {{ data.valuePreview }}
                  </span>
                </span>
              </template>
            </ElTree>
          </div>

          <div class="helper-actions mt-3">
            <ElButton size="small" @click="step = 'input'">
              重新粘贴 JSON
            </ElButton>
          </div>
        </div>
      </div>
    </ElPopover>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { ElMessage } from "element-plus";

interface Props {
  plain?: boolean;
  disabled?: boolean;
  seedJson?: string;
}

const props = withDefaults(defineProps<Props>(), {
  plain: false,
  disabled: false,
  seedJson: "",
});

const emit = defineEmits<{
  (e: "select", path: string): void;
}>();

const visible = ref(false);
const step = ref<"input" | "tree">("input");
const rawJsonInput = ref("");

interface TreeNode {
  name: string;
  jsonPath: string;
  valuePreview?: string;
  children?: TreeNode[];
}

const treeData = ref<TreeNode[]>([]);

const resetHelper = () => {
  step.value = "input";
  rawJsonInput.value = props.seedJson || "";
  treeData.value = [];
  if (rawJsonInput.value.trim()) {
    try {
      const parsed = JSON.parse(rawJsonInput.value.trim());
      if (typeof parsed === "object" && parsed !== null) {
        treeData.value = valueToTreeNodes(parsed, "$");
        step.value = "tree";
      }
    } catch {
      void 0;
    }
  }
};

const valueToTreeNodes = (val: any, path: string = "$"): TreeNode[] => {
  if (val === null) {
    return [{ name: "null", jsonPath: path, valuePreview: "null" }];
  }

  if (Array.isArray(val)) {
    return val.map((item, idx) => {
      const currentPath = `${path}[${idx}]`;
      const isPrimitive = typeof item !== "object" || item === null;
      return {
        name: `[${idx}]`,
        jsonPath: currentPath,
        valuePreview: isPrimitive ? String(item) : undefined,
        children: isPrimitive ? undefined : valueToTreeNodes(item, currentPath),
      };
    });
  }

  if (typeof val === "object") {
    return Object.entries(val).map(([key, item]) => {

      const needsBrackets = /[^a-zA-Z0-9_]/.test(key);
      const currentPath = needsBrackets
        ? `${path}['${key}']`
        : `${path}.${key}`;
      const isPrimitive = typeof item !== "object" || item === null;
      return {
        name: key,
        jsonPath: currentPath,
        valuePreview: isPrimitive ? String(item) : undefined,
        children: isPrimitive ? undefined : valueToTreeNodes(item, currentPath),
      };
    });
  }

  return [];
};

const parseJson = () => {
  try {
    const parsed = JSON.parse(rawJsonInput.value.trim());
    if (typeof parsed !== "object" || parsed === null) {
      ElMessage.warning("请粘贴有效 JSON");
      return;
    }

    const nodes = valueToTreeNodes(parsed, "$");
    treeData.value = nodes;
    step.value = "tree";
  } catch (e) {
    ElMessage.error("JSON 格式有误");
  }
};

const handleNodeClick = (data: TreeNode) => {
  if (data.jsonPath) {
    emit("select", data.jsonPath);
    visible.value = false;
  }
};
</script>

<style scoped lang="scss">
.json-path-helper {
  display: inline-block;

  .helper-trigger {
    cursor: pointer;
    color: var(--el-color-primary);
    font-size: 12px;
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    background-color: var(--el-color-primary-light-9);
    border-radius: var(--el-border-radius-base);
    transition: all 0.2s;
    user-select: none;
    font-weight: 500;

    &:hover {
      background-color: var(--el-color-primary-light-8);
      transform: translateY(-1px);
    }
  }

  .helper-trigger-plain {
    cursor: pointer;
    color: var(--el-color-primary);
    font-size: 12px;
    display: inline-flex;
    align-items: center;
    user-select: none;
    font-weight: 500;
    transition: all 0.2s;

    &:hover {
      color: var(--el-color-primary-light-3);
    }
  }

  .is-disabled {
    cursor: not-allowed !important;
    color: var(--el-text-color-placeholder) !important;
    pointer-events: none;
    transform: none !important;

    &:not(.helper-trigger-plain) {
      background-color: var(--el-fill-color-light) !important;
    }
    &.helper-trigger-plain {
      background-color: transparent !important;
    }
  }
}

.helper-content {
  padding: 4px;

  .helper-title {
    font-size: 13px;
    font-weight: 600;
    margin-top: 0;
    margin-bottom: 8px;
    color: var(--el-text-color-primary);
  }

  .helper-intro-text {
    font-size: 11px;
    color: var(--el-text-color-secondary);
    line-height: 1.4;
    margin-top: 0;
    margin-bottom: 10px;
  }

  .step-input,
  .step-tree {
    display: flex;
    flex-direction: column;
  }

  .tree-container {
    max-height: 220px;
    overflow-y: auto;
    border: 1px solid var(--el-border-color-light);
    border-radius: var(--el-border-radius-base);
    padding: 4px;
    background-color: var(--el-fill-color-blank);

    .custom-tree-node {
      font-size: 12px;
      display: inline-flex;
      align-items: center;

      .node-label {
        font-family: monospace;
        font-weight: 600;
        color: var(--el-text-color-primary);
      }

      .node-preview {
        color: var(--el-text-color-secondary);
        margin-left: 4px;
        max-width: 180px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-style: italic;
      }
    }
  }

  .helper-actions {
    display: flex;
    justify-content: flex-end;
  }
}
</style>
