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
    var danger = cssVar("--color-error", "#ffb4ab");

    // Multi-series payloads render as lines; the single-series shape stays a bar
    // chart, so existing callers are untouched.
    var multi = Array.isArray(data.series);
    var lineColors = [muted, primary];

    var datasets = multi
      ? data.series.map(function (s, i) {
          var color = lineColors[i % lineColors.length];
          var marks = {};
          (s.markers || []).forEach(function (m) {
            marks[m] = true;
          });
          return {
            label: s.name || "",
            data: s.values,
            borderColor: color,
            backgroundColor: color,
            borderWidth: 2,
            tension: 0.25,
            pointRadius: s.values.map(function (_, j) {
              return marks[j] ? 5 : 2;
            }),
            pointBackgroundColor: s.values.map(function (_, j) {
              return marks[j] ? danger : color;
            }),
            pointBorderColor: s.values.map(function (_, j) {
              return marks[j] ? danger : color;
            }),
          };
        })
      : [
          {
            label: data.name || "",
            data: data.values,
            backgroundColor: primary,
            hoverBackgroundColor: primary,
            borderRadius: 3,
            maxBarThickness: 46,
          },
        ];

    var chart = new Chart(canvas.getContext("2d"), {
      type: multi ? "line" : "bar",
      data: {
        labels: data.labels,
        datasets: datasets,
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        animation: { duration: 250 },
        interaction: multi ? { mode: "index", intersect: false } : undefined,
        plugins: {
          legend: multi
            ? { display: true, position: "bottom", labels: { color: muted, boxHeight: 2 } }
            : { display: false },
          tooltip: {
            backgroundColor: surface,
            titleColor: onSurface,
            bodyColor: onSurface,
            borderColor: grid,
            borderWidth: 1,
            padding: 8,
            displayColors: multi,
            callbacks: data.unit
              ? {
                  label: function (ctx) {
                    return ctx.dataset.label + ": " + ctx.formattedValue + " " + data.unit;
                  },
                }
              : {},
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
