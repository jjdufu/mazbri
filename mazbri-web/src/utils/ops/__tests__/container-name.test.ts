import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  containerDisplayName,
  withContainerDisplayName,
} from "../container-name";

describe("containerDisplayName", () => {
  it("prefers compose service over name and id", () => {
    assert.equal(
      containerDisplayName({
        container_label_com_docker_compose_service: "mazbri-gateway",
        name: "/noisy",
        id: "/docker/abcdef0123456789",
      }),
      "mazbri-gateway",
    );
  });

  it("uses k8s container then pod", () => {
    assert.equal(
      containerDisplayName({
        container_label_io_kubernetes_container_name: "app",
        container_label_io_kubernetes_pod_name: "app-0",
        id: "/kubepods/podx/abc",
      }),
      "app",
    );
    assert.equal(
      containerDisplayName({
        container_label_io_kubernetes_pod_name: "app-0",
        id: "/kubepods/podx/abc",
      }),
      "app-0",
    );
  });

  it("uses image repo name when docker name is a hex id", () => {
    assert.equal(
      containerDisplayName({
        name: "10a1bc3773b27a96ad559c9b43bd606f1608f7ab4489e426d0140d27aad712d5",
        image: "docker.io/dex2all/mazbri-gateway:0.2.11",
        id: "/docker/10a1bc3773b27a96ad559c9b43bd606f1608f7ab4489e426d0140d27aad712d5",
      }),
      "mazbri-gateway",
    );
  });

  it("falls back to docker name then short id", () => {
    assert.equal(
      containerDisplayName({ name: "/mazbri-mgmt-1", id: "/docker/deadbeefcafe" }),
      "mazbri-mgmt-1",
    );
    assert.equal(
      containerDisplayName({ id: "/docker/deadbeefcafebabe0123" }),
      "deadbeefcafe",
    );
    assert.equal(containerDisplayName({}), "unknown");
  });

  it("remaps series labels for charts", () => {
    const out = withContainerDisplayName([
      {
        labels: {
          name: "x",
          container_label_com_docker_compose_service: "vector",
          id: "/docker/1",
        },
        points: [],
      },
    ]);
    assert.deepEqual(out[0].labels, { container: "vector" });
  });
});
