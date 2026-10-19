<template>
  <canvas
    ref="canvasRef"
    class="fixed top-0 left-0 z-[9999] w-full h-full pointer-events-none"
  ></canvas>
</template>

<script setup lang="ts">
import { useEventListener } from "@vueuse/core";
import { ErrorCodes } from "@/constants/error-codes";
import { mittBus } from "@/utils/sys";
import { logger } from "@/utils/sys";
import type { Handler } from "mitt";
import bp from "@/assets/images/ceremony/hb.png";
import sd from "@/assets/images/ceremony/sd.png";
import yd from "@/assets/images/ceremony/yd.png";
defineOptions({ name: "ArtFireworksEffect" });
interface FireworkConfig {
  readonly POOL_SIZE: number;
  readonly PARTICLES_PER_BURST: number;
  readonly SIZES: {
    readonly RECTANGLE: {
      readonly WIDTH: number;
      readonly HEIGHT: number;
    };
    readonly SQUARE: {
      readonly SIZE: number;
    };
    readonly CIRCLE: {
      readonly SIZE: number;
    };
    readonly TRIANGLE: {
      readonly SIZE: number;
    };
    readonly OVAL: {
      readonly WIDTH: number;
      readonly HEIGHT: number;
    };
    readonly IMAGE: {
      readonly WIDTH: number;
      readonly HEIGHT: number;
    };
  };
  readonly ROTATION: {
    readonly BASE_SPEED: number;
    readonly RANDOM_SPEED: number;
    readonly DECAY: number;
  };
  readonly PHYSICS: {
    readonly GRAVITY: number;
    readonly VELOCITY_THRESHOLD: number;
    readonly OPACITY_DECAY: number;
  };
  readonly COLORS: readonly string[];
  readonly SHAPES: readonly string[];
}
interface Firework {
  x: number;
  y: number;
  vx: number;
  vy: number;
  color: string;
  rotation: number;
  rotationSpeed: number;
  scale: number;
  shape: string;
  opacity: number;
  active: boolean;
  imageUrl?: string;
}
interface ImageCache {
  [url: string]: HTMLImageElement;
}
const CONFIG: FireworkConfig = {
  POOL_SIZE: 600,
  PARTICLES_PER_BURST: 200,
  SIZES: {
    RECTANGLE: { WIDTH: 24, HEIGHT: 12 },
    SQUARE: { SIZE: 12 },
    CIRCLE: { SIZE: 12 },
    TRIANGLE: { SIZE: 10 },
    OVAL: { WIDTH: 24, HEIGHT: 12 },
    IMAGE: { WIDTH: 30, HEIGHT: 30 },
  },
  ROTATION: {
    BASE_SPEED: 2,
    RANDOM_SPEED: 3,
    DECAY: 0.98,
  },
  PHYSICS: {
    GRAVITY: 0.525,
    VELOCITY_THRESHOLD: 10,
    OPACITY_DECAY: 0.02,
  },
  COLORS: [
    "rgba(255, 68, 68, 1)",
    "rgba(255, 68, 68, 0.9)",
    "rgba(255, 68, 68, 0.8)",
    "rgba(255, 116, 188, 1)",
    "rgba(255, 116, 188, 0.9)",
    "rgba(255, 116, 188, 0.8)",
    "rgba(68, 68, 255, 0.8)",
    "rgba(92, 202, 56, 0.7)",
    "rgba(255, 68, 255, 0.8)",
    "rgba(68, 255, 255, 0.7)",
    "rgba(255, 136, 68, 0.7)",
    "rgba(68, 136, 255, 1)",
    "rgba(250, 198, 122, 0.8)",
  ],
  SHAPES: [
    "rectangle",
    "rectangle",
    "rectangle",
    "rectangle",
    "rectangle",
    "rectangle",
    "rectangle",
    "circle",
    "triangle",
    "oval",
  ],
} as const;
const canvasRef = ref<HTMLCanvasElement>();
const ctx = ref<CanvasRenderingContext2D | null>(null);
class FireworkSystem {
  private particlePool: Firework[] = [];
  private activeParticles: Firework[] = [];
  private poolIndex = 0;
  private imageCache: ImageCache = {};
  private animationId = 0;
  private canvasWidth = 0;
  private canvasHeight = 0;
  constructor() {
    this.initializePool();
  }
  private initializePool(): void {
    for (let i = 0; i < CONFIG.POOL_SIZE; i++) {
      this.particlePool.push(this.createParticle());
    }
  }
  private createParticle(): Firework {
    return {
      x: 0,
      y: 0,
      vx: 0,
      vy: 0,
      color: "",
      rotation: 0,
      rotationSpeed: 0,
      scale: 1,
      shape: "circle",
      opacity: 1,
      active: false,
    };
  }
  private getAvailableParticle(): Firework | null {
    for (let i = 0; i < CONFIG.POOL_SIZE; i++) {
      const index = (this.poolIndex + i) % CONFIG.POOL_SIZE;
      const particle = this.particlePool[index];
      if (!particle.active) {
        this.poolIndex = (index + 1) % CONFIG.POOL_SIZE;
        particle.active = true;
        return particle;
      }
    }
    return null;
  }
  async preloadImage(url: string): Promise<HTMLImageElement> {
    if (this.imageCache[url]) {
      return this.imageCache[url];
    }
    return new Promise((resolve, reject) => {
      const img = new Image();
      img.crossOrigin = "anonymous";
      img.onload = () => {
        this.imageCache[url] = img;
        resolve(img);
      };
      img.onerror = reject;
      img.src = url;
    });
  }
  async preloadAllImages(): Promise<void> {
    const imageUrls = [bp, sd, yd];
    try {
      await Promise.all(imageUrls.map((url) => this.preloadImage(url)));
    } catch (error) {
      logger.error("Fireworks image preloading failed", {
        service: "mazbri-mgmt-web",
        event: "fireworks_image_preload_failed",
        severity: "error",
        error_code: ErrorCodes.INTERNAL_ERROR,
        error,
      });
    }
  }
  createFirework(imageUrl?: string): void {
    const startX = Math.random() * this.canvasWidth;
    const startY = this.canvasHeight;
    const availableShapes =
      imageUrl && this.imageCache[imageUrl] ? ["image"] : CONFIG.SHAPES;
    const particles: Firework[] = [];
    for (let i = 0; i < CONFIG.PARTICLES_PER_BURST; i++) {
      const particle = this.getAvailableParticle();
      if (!particle) continue;
      const angle = (Math.PI * i) / (CONFIG.PARTICLES_PER_BURST / 2);
      const speed = (12 + Math.random() * 6) * 1.5;
      const spread = Math.random() * Math.PI * 2;
      particle.x = startX;
      particle.y = startY;
      particle.vx =
        Math.cos(angle) *
        Math.cos(spread) *
        speed *
        (Math.random() * 0.5 + 0.5);
      particle.vy = Math.sin(angle) * speed - 15;
      particle.color =
        CONFIG.COLORS[Math.floor(Math.random() * CONFIG.COLORS.length)];
      particle.rotation = Math.random() * 360;
      particle.rotationSpeed =
        (Math.random() * CONFIG.ROTATION.RANDOM_SPEED +
          CONFIG.ROTATION.BASE_SPEED) *
        (Math.random() > 0.5 ? 1 : -1);
      particle.scale = 0.8 + Math.random() * 0.4;
      particle.shape =
        availableShapes[Math.floor(Math.random() * availableShapes.length)];
      particle.opacity = 1;
      particle.imageUrl =
        imageUrl && this.imageCache[imageUrl] ? imageUrl : undefined;
      particles.push(particle);
    }
    this.activeParticles.push(...particles);
  }
  private updateParticles(): void {
    const { GRAVITY, VELOCITY_THRESHOLD, OPACITY_DECAY } = CONFIG.PHYSICS;
    const { DECAY } = CONFIG.ROTATION;
    for (let i = this.activeParticles.length - 1; i >= 0; i--) {
      const particle = this.activeParticles[i];
      particle.x += particle.vx;
      particle.y += particle.vy;
      particle.vy += GRAVITY;
      particle.rotation += particle.rotationSpeed;
      particle.rotationSpeed *= DECAY;
      if (particle.vy > VELOCITY_THRESHOLD) {
        particle.opacity -= OPACITY_DECAY;
        if (particle.opacity <= 0) {
          this.recycleParticle(i);
          continue;
        }
      }
      if (this.isOutOfBounds(particle)) {
        this.recycleParticle(i);
      }
    }
  }
  private recycleParticle(index: number): void {
    const particle = this.activeParticles[index];
    particle.active = false;
    this.activeParticles.splice(index, 1);
  }
  private isOutOfBounds(particle: Firework): boolean {
    const margin = 100;
    return (
      particle.x < -margin ||
      particle.x > this.canvasWidth + margin ||
      particle.y < -margin ||
      particle.y > this.canvasHeight + margin
    );
  }
  private drawParticle(particle: Firework): void {
    if (!ctx.value) return;
    ctx.value.save();
    ctx.value.globalAlpha = particle.opacity;
    ctx.value.translate(particle.x, particle.y);
    ctx.value.rotate((particle.rotation * Math.PI) / 180);
    ctx.value.scale(particle.scale, particle.scale);
    this.renderShape(particle);
    ctx.value.restore();
  }
  private renderShape(particle: Firework): void {
    if (!ctx.value) return;
    const { SIZES } = CONFIG;
    ctx.value.fillStyle = particle.color;
    switch (particle.shape) {
      case "rectangle":
        ctx.value.fillRect(
          -SIZES.RECTANGLE.WIDTH / 2,
          -SIZES.RECTANGLE.HEIGHT / 2,
          SIZES.RECTANGLE.WIDTH,
          SIZES.RECTANGLE.HEIGHT,
        );
        break;
      case "square":
        ctx.value.fillRect(
          -SIZES.SQUARE.SIZE / 2,
          -SIZES.SQUARE.SIZE / 2,
          SIZES.SQUARE.SIZE,
          SIZES.SQUARE.SIZE,
        );
        break;
      case "circle":
        ctx.value.beginPath();
        ctx.value.arc(0, 0, SIZES.CIRCLE.SIZE / 2, 0, Math.PI * 2);
        ctx.value.fill();
        break;
      case "triangle":
        ctx.value.beginPath();
        ctx.value.moveTo(0, -SIZES.TRIANGLE.SIZE);
        ctx.value.lineTo(SIZES.TRIANGLE.SIZE, SIZES.TRIANGLE.SIZE);
        ctx.value.lineTo(-SIZES.TRIANGLE.SIZE, SIZES.TRIANGLE.SIZE);
        ctx.value.closePath();
        ctx.value.fill();
        break;
      case "oval":
        ctx.value.beginPath();
        ctx.value.ellipse(
          0,
          0,
          SIZES.OVAL.WIDTH / 2,
          SIZES.OVAL.HEIGHT / 2,
          0,
          0,
          Math.PI * 2,
        );
        ctx.value.fill();
        break;
      case "image":
        this.renderImage(particle);
        break;
    }
  }
  private renderImage(particle: Firework): void {
    if (!ctx.value || !particle.imageUrl) return;
    const img = this.imageCache[particle.imageUrl];
    if (img?.complete) {
      const { WIDTH, HEIGHT } = CONFIG.SIZES.IMAGE;
      ctx.value.drawImage(img, -WIDTH / 2, -HEIGHT / 2, WIDTH, HEIGHT);
    }
  }
  private render(): void {
    if (!ctx.value || !canvasRef.value) return;
    ctx.value.clearRect(0, 0, this.canvasWidth, this.canvasHeight);
    ctx.value.globalCompositeOperation = "lighter";
    for (const particle of this.activeParticles) {
      this.drawParticle(particle);
    }
  }
  private animate = (): void => {
    this.updateParticles();
    this.render();
    this.animationId = requestAnimationFrame(this.animate);
  };
  updateCanvasSize(width: number, height: number): void {
    this.canvasWidth = width;
    this.canvasHeight = height;
  }
  start(): void {
    this.animate();
  }
  stop(): void {
    if (this.animationId) {
      cancelAnimationFrame(this.animationId);
      this.animationId = 0;
    }
  }
  getActiveParticleCount(): number {
    return this.activeParticles.length;
  }
}
const fireworkSystem = new FireworkSystem();
const handleKeyPress = (event: KeyboardEvent): void => {
  const isFireworkShortcut =
    (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "p") ||
    (event.metaKey && event.shiftKey && event.key.toLowerCase() === "p");
  if (isFireworkShortcut) {
    event.preventDefault();
    fireworkSystem.createFirework();
  }
};
const resizeCanvas = (): void => {
  if (!canvasRef.value) return;
  const { innerWidth, innerHeight } = window;
  canvasRef.value.width = innerWidth;
  canvasRef.value.height = innerHeight;
  fireworkSystem.updateCanvasSize(innerWidth, innerHeight);
};
const handleFireworkTrigger: Handler<unknown> = (event: unknown) => {
  const imageUrl = event as string | undefined;
  fireworkSystem.createFirework(imageUrl);
};
onMounted(async () => {
  if (!canvasRef.value) return;
  ctx.value = canvasRef.value.getContext("2d");
  if (!ctx.value) return;
  resizeCanvas();
  await fireworkSystem.preloadAllImages();
  fireworkSystem.start();
  useEventListener(window, "keydown", handleKeyPress);
  useEventListener(window, "resize", resizeCanvas);
  mittBus.on("triggerFireworks", handleFireworkTrigger);
});
onUnmounted(() => {
  fireworkSystem.stop();
  mittBus.off("triggerFireworks", handleFireworkTrigger);
});
</script>
