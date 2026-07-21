// Client-side rendering of the larger time-series bar charts. The server embeds
// each chart's data as JSON on a <canvas data-chart="...">; we read the theme
// colors from the CSS custom properties so the chart matches light/dark, then
// let Chart.js redraw crisply at any width. (The old server-rendered SVGs scaled
// their fixed viewBox and distorted when the container grew.)
(function () {
  if (typeof Chart === "undefined") return;

  Chart.defaults.font.family = "Inter, system-ui, sans-serif";

  function cssVar(name, fallback) {
    var v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    return v || fallback;
  }

  var instances = [];

  function build(canvas) {
    var raw = canvas.dataset.chart;
    if (!raw) return;
    var data;
    try {
      data = JSON.parse(raw);
    } catch (e) {
      return;
    }

    var primary = cssVar("--color-primary", "#c0c1ff");
    var muted = cssVar("--color-on-surface-variant", "#9ca3af");
    var grid = cssVar("--color-outline-variant", "#464554");
    var surface = cssVar("--color-surface-container-high", "#2a2a33");
    var onSurface = cssVar("--color-on-surface", "#e6e6e6");

    var chart = new Chart(canvas.getContext("2d"), {
      type: "bar",
      data: {
        labels: data.labels,
        datasets: [
          {
            label: data.name || "",
            data: data.values,
            backgroundColor: primary,
            hoverBackgroundColor: primary,
            borderRadius: 3,
            maxBarThickness: 46,
          },
        ],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        animation: { duration: 250 },
        plugins: {
          legend: { display: false },
          tooltip: {
            backgroundColor: surface,
            titleColor: onSurface,
            bodyColor: onSurface,
            borderColor: grid,
            borderWidth: 1,
            padding: 8,
            displayColors: false,
          },
        },
        scales: {
          x: {
            grid: { display: false },
            border: { color: grid },
            ticks: {
              color: muted,
              font: { size: 10 },
              maxRotation: 45,
              autoSkip: true,
              autoSkipPadding: 8,
            },
          },
          y: {
            beginAtZero: true,
            grid: { color: grid, drawTicks: false },
            border: { display: false },
            ticks: { color: muted, font: { size: 10 }, precision: 0, maxTicksLimit: 5 },
          },
        },
      },
    });
    instances.push(chart);
  }

  function renderAll() {
    instances.forEach(function (c) {
      c.destroy();
    });
    instances = [];
    document.querySelectorAll("canvas[data-chart]").forEach(build);
  }

  renderAll();

  // Rebuild with the fresh palette when the OS theme flips.
  var mq = window.matchMedia("(prefers-color-scheme: dark)");
  if (mq.addEventListener) {
    mq.addEventListener("change", renderAll);
  }
})();
