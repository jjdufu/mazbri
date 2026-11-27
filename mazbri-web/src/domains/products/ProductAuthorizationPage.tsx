import { useEffect, useState } from "react";
import { useSearchParams } from "react-router";
import { Tabs, type TabOption } from "@/components/ui";
import { useT } from "@/locales";
import { AuthorizationListTab } from "./AuthorizationListTab";
import { ProductListTab } from "./ProductListTab";

type ProductAuthorizationTab = "products" | "authorizations";

function resolveTab(raw: string | null): ProductAuthorizationTab {
  return raw === "authorizations" ? "authorizations" : "products";
}

export function ProductAuthorizationPage() {
  const { t } = useT();
  const [searchParams, setSearchParams] = useSearchParams();
  const activeTab = resolveTab(searchParams.get("tab"));
  const [loadedTabs, setLoadedTabs] = useState<Set<ProductAuthorizationTab>>(
    () => new Set([activeTab]),
  );

  useEffect(() => {
    setLoadedTabs((prev) => {
      if (prev.has(activeTab)) return prev;
      const next = new Set(prev);
      next.add(activeTab);
      return next;
    });
  }, [activeTab]);

  const handleTabChange = (next: ProductAuthorizationTab) => {
    const nextParams = new URLSearchParams(searchParams);
    nextParams.set("tab", next);
    setSearchParams(nextParams, { replace: true });
  };

  const tabItems: TabOption<ProductAuthorizationTab>[] = [
    {
      key: "products",
      label: t("menus.mazbri.productList"),
      testId: "tab-products",
    },
    {
      key: "authorizations",
      label: t("menus.mazbri.authorizationList"),
      testId: "tab-authorizations",
    },
  ];

  return (
    <div className="space-y-4 min-w-0" data-testid="product-authorization-page">
      <Tabs items={tabItems} value={activeTab} onChange={handleTabChange} />

      {loadedTabs.has("products") && (
        <div className={activeTab === "products" ? "min-w-0" : "hidden"}>
          <ProductListTab />
        </div>
      )}

      {loadedTabs.has("authorizations") && (
        <div className={activeTab === "authorizations" ? "min-w-0" : "hidden"}>
          <AuthorizationListTab />
        </div>
      )}
    </div>
  );
}
