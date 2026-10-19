import type { Ref } from "vue";

export async function runWithLoading<T>(
  loading: Ref<boolean>,
  task: () => Promise<T>,
): Promise<T> {
  loading.value = true;
  try {
    return await task();
  } finally {
    loading.value = false;
  }
}
