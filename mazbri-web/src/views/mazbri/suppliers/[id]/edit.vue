<template>
  <div class="supplier-edit-redirect" />
</template>

<script setup lang="ts">




import { onMounted } from "vue";
import { useRoute, useRouter } from "vue-router";

defineOptions({ name: "SupplierEdit" });

const route = useRoute();
const router = useRouter();

onMounted(() => {
  router.replace({
    name: "SupplierDetail",
    params: { id: route.params.id as string },
    query: { edit: "1" },
  });
});
</script>
