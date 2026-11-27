import { useEffect, useMemo, useState } from "react";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { useWatermarkStore } from "@/stores/watermark";
import { formatDateTime } from "@/utils/format";

const GAP_X = 280;
const GAP_Y = 140;
const ROTATE_DEG = -22;

export function WatermarkOverlay() {
  const { t } = useT();
  const visible = useWatermarkStore((s) => s.visible);
  const username = useAuthStore(
    (s) => s.info.username || s.info.userName || "unknown",
  );
  const [timestamp, setTimestamp] = useState<string>(() =>
    formatDateTime(Date.now()).replace(/-/g, "/"),
  );
  const [dimensions, setDimensions] = useState<{
    width: number;
    height: number;
  }>(() => ({
    width: typeof window !== "undefined" ? window.innerWidth : 1440,
    height: typeof window !== "undefined" ? window.innerHeight : 900,
  }));

  useEffect(() => {
    if (!visible) return;
    const tick = () => {
      setTimestamp(formatDateTime(Date.now()).replace(/-/g, "/"));
    };
    tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  }, [visible]);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const onResize = () => {
      setDimensions({
        width: window.innerWidth,
        height: window.innerHeight,
      });
    };
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  const positions = useMemo(() => {
    const result: Array<{ id: number; x: number; y: number }> = [];
    const cols = Math.ceil(dimensions.width / GAP_X) + 2;
    const rows = Math.ceil(dimensions.height / GAP_Y) + 2;
    let id = 0;
    for (let row = -1; row < rows; row++) {
      for (let col = -1; col < cols; col++) {
        result.push({
          id: id++,
          x: col * GAP_X + (row % 2 === 0 ? 0 : GAP_X / 2),
          y: row * GAP_Y,
        });
      }
    }
    return result;
  }, [dimensions.height, dimensions.width]);

  if (!visible) return null;

  const watermarkText = `${username} ${timestamp}`;

  return (
    <div
      data-testid="watermark-overlay"
      aria-hidden="true"
      className="pointer-events-none fixed inset-0 z-[60] overflow-hidden select-none"
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox={`0 0 ${dimensions.width} ${dimensions.height}`}
        preserveAspectRatio="xMidYMid slice"
        className="h-full w-full overflow-visible"
      >
        <title>{t("watermark.title")}</title>
        <g
          transform={`rotate(${ROTATE_DEG}, ${dimensions.width / 2}, ${dimensions.height / 2})`}
        >
          {positions.map((pos) => (
            <text
              key={pos.id}
              x={pos.x}
              y={pos.y}
              fill="rgba(128, 128, 128, 0.15)"
              fontSize={14}
              fontFamily="system-ui, -apple-system, sans-serif"
              textAnchor="middle"
              dominantBaseline="middle"
            >
              {watermarkText}
            </text>
          ))}
        </g>
      </svg>
    </div>
  );
}
