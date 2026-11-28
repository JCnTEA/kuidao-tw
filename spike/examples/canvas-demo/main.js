function refresh() {
    return {
        status: {
            state: "succeeded",
            label: "畫布圖元",
            summary: "這行是與畫布混排的標準狀態組件。"
        },
        canvas: {
            height: 172,
            nodes: [
                { text: "趨勢與進度", at: [0, 24], size: 20, weight: 700, font: "站酷快樂體", fill: "$text" },
                { image: "badge.svg", rect: [348, 0, 34, 30], fit: "contain" },
                { rect: [0, 38, 226, 129], r: [12, 12, 8, 8], fill: "$card", shadow: "sm" },
                { rect: [236, 38, 151, 129], r: 12, fill: "$card" },
                { text: "面積與折線", at: [12, 60], size: 12, fill: "$muted" },
                {
                    translate: [12, 72],
                    clip: [0, 0, 202, 79],
                    group: [
                        { line: [0, 33, 202, 33], stroke: "$track", dash: [4, 4] },
                        {
                            points: [[0, 62], [34, 49], [68, 52], [103, 24], [136, 32], [169, 18], [202, 8]],
                            area: 79,
                            fill: { linear: [[0, 0], [0, 79]], stops: [[0, "#46BA8D70"], [1, "#46BA8D00"]] },
                            stroke: "$accent",
                            width: 2,
                            cap: "round"
                        },
                        { circle: [198, 9, 3], fill: "$accent", glow: true }
                    ]
                },
                { text: "圓弧與路徑", at: [248, 60], size: 12, fill: "$muted" },
                {
                    circle: [311, 110, 42],
                    fill: { radial: [[311, 110], 42], stops: [[0, "#46BA8D32"], [1, "#46BA8D00"]] }
                },
                { arc: { c: [311, 110], r: 32, start: 135, sweep: 270 }, stroke: "$track", width: 5, cap: "round" },
                { arc: { c: [311, 110], r: 32, start: 135, sweep: 194.4 }, stroke: "$accent", width: 5, cap: "round" },
                { text: "72%", at: [311, 116], size: 19, weight: 700, align: "center", max_width: 54, lines: 1 },
                {
                    group: [{ path: "M0 6 L4 10 Q5 11 6 9 L13 1", stroke: "$accent", width: 2, cap: "round" }],
                    translate: [304, 136]
                }
            ]
        }
    };
}
