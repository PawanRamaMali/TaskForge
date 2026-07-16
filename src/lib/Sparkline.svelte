<script lang="ts">
  let {
    data = [] as number[],
    max = 100,
    color = '#4f9cf9',
    height = 36,
  } = $props();

  let canvas: HTMLCanvasElement | undefined = $state();

  $effect(() => {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const cssWidth = canvas.clientWidth || 120;
    const cssHeight = height;
    canvas.width = cssWidth * dpr;
    canvas.height = cssHeight * dpr;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, cssWidth, cssHeight);
    if (data.length < 2) return;

    const effectiveMax = Math.max(max, ...data) || 1;
    const stepX = cssWidth / (60 - 1);
    const toX = (i: number) => cssWidth - (data.length - 1 - i) * stepX;
    const toY = (v: number) => cssHeight - (v / effectiveMax) * (cssHeight - 2) - 1;

    ctx.beginPath();
    ctx.moveTo(toX(0), toY(data[0]));
    for (let i = 1; i < data.length; i++) ctx.lineTo(toX(i), toY(data[i]));
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5;
    ctx.stroke();

    ctx.lineTo(toX(data.length - 1), cssHeight);
    ctx.lineTo(toX(0), cssHeight);
    ctx.closePath();
    ctx.fillStyle = color + '2a';
    ctx.fill();
  });
</script>

<canvas bind:this={canvas} style="width: 100%; height: {height}px;"></canvas>
