import SwiftUI

// Sprites and colour hashing after MonoCode's project mascots (MIT), so a
// project keeps one face and colour wherever it shows.

/// 8×8 pixel mascots that stand for a project. `#` paints the project's
/// colour; `.` stays clear, so eyes and gaps are holes. The `talk` frame swaps
/// in while the project has a turn in flight.
struct ProjectMascotSprite: Sendable {
    let name: String
    let rest: [String]
    let talk: [String]

    static let all: [ProjectMascotSprite] = [
        .init(
            name: "invader",
            rest: ["..#..#..", ".######.", "##.##.##", "########", ".######.", ".#.##.#.", "#.#..#.#", "........"],
            talk: ["..#..#..", ".######.", "##.##.##", "########", ".######.", "#.####.#", ".#....#.", "#......#"]
        ),
        .init(
            name: "ghost",
            rest: ["..####..", ".######.", "##.##.##", "########", "########", "########", "########", "#.##.##."],
            talk: ["..####..", ".######.", "#.##.###", "########", "########", "########", "########", ".##.##.#"]
        ),
        .init(
            name: "robot",
            rest: ["...#....", ".######.", ".#.##.#.", ".######.", ".#....#.", ".######.", "..#..#..", "........"],
            talk: ["....#...", ".######.", ".#.##.#.", ".######.", ".##..##.", ".######.", ".#....#.", "........"]
        ),
        .init(
            name: "cat",
            rest: [".#....#.", ".##..##.", "########", "#.####.#", "########", "###..###", ".######.", "..#..#.."],
            talk: [".#....#.", ".##..##.", "########", "#.####.#", "########", "########", ".######.", ".#....#."]
        ),
        .init(
            name: "skull",
            rest: [".######.", "########", "##.##.##", "########", ".##..##.", ".######.", ".#.##.#.", "........"],
            talk: [".######.", "########", "##.##.##", "########", ".##..##.", ".######.", ".#....#.", "..####.."]
        ),
        .init(
            name: "crab",
            rest: ["#......#", ".#....#.", ".######.", "##.##.##", "########", "#.####.#", "#......#", "........"],
            talk: ["#......#", "##....##", ".######.", "##.##.##", "########", ".######.", "#.#..#.#", "........"]
        ),
        .init(
            name: "mushroom",
            rest: ["..####..", ".######.", "########", "##.##.##", "########", "...##...", "...##...", "..####.."],
            talk: ["........", "..####..", ".######.", "########", "##.##.##", "...##...", "...##...", "..####.."]
        ),
        .init(
            name: "rocket",
            rest: ["...##...", "..####..", "..#..#..", "..####..", ".######.", ".######.", "##....##", "..####.."],
            talk: ["...##...", "..####..", "..#..#..", "..####..", ".######.", ".######.", "##....##", "...##..."]
        ),
        .init(
            name: "dino",
            rest: ["...#####", "...##.##", "...#####", ".#######", "########", "#####...", ".##.##..", "..#..#.."],
            talk: ["...#####", "...##.##", "...#####", ".#######", "########", "#####...", "..##.##.", "..#...#."]
        ),
        .init(
            name: "frog",
            rest: ["........", "##....##", "#.####.#", "########", "########", ".######.", "##....##", "........"],
            talk: ["##....##", "#.####.#", "########", "########", ".######.", "##....##", "#......#", "........"]
        ),
    ]

    /// A stable pick per project, mixed differently from its colour so the two vary apart.
    static func forProject(_ path: String) -> ProjectMascotSprite {
        var hash: UInt32 = 0
        for unit in path.utf16 { hash = hash &* 131 &+ UInt32(unit) }
        return all[Int(hash % UInt32(all.count))]
    }

    /// Each row's filled runs, so a frame draws as a few rects.
    func runs(talking: Bool) -> [CGRect] {
        var rects: [CGRect] = []
        for (y, row) in (talking ? talk : rest).enumerated() {
            let cells = Array(row)
            var x = 0
            while x < cells.count {
                guard cells[x] == "#" else { x += 1; continue }
                var width = 1
                while x + width < cells.count, cells[x + width] == "#" { width += 1 }
                rects.append(CGRect(x: x, y: y, width: width, height: 1))
                x += width
            }
        }
        return rects
    }
}

enum ProjectColors {
    /// The desktop's tab-group colours; the neutral first is skipped for projects.
    static let all: [Color] = [
        hsla(210, 8, 58), hsla(211, 92, 62), hsla(12, 80, 58), hsla(45, 90, 55), hsla(142, 55, 50),
        hsla(330, 70, 62), hsla(280, 55, 62), hsla(175, 55, 48), hsla(25, 85, 58),
    ]

    static func forProject(_ path: String) -> Color {
        var hash: UInt32 = 0
        for unit in path.utf16 { hash = hash &* 31 &+ UInt32(unit) }
        return all[Int(hash % UInt32(all.count - 1)) + 1]
    }

    private static func hsla(_ hue: Double, _ saturation: Double, _ lightness: Double) -> Color {
        Color(hex: hsl(hue, saturation, lightness))
    }
}

/// A project's mascot in its colour, its two frames cycling while `active`.
struct ProjectMascot: View {
    let path: String
    var size: CGFloat = 20
    var active = false

    var body: some View {
        let sprite = ProjectMascotSprite.forProject(path)
        let color = ProjectColors.forProject(path)
        TimelineView(.periodic(from: .now, by: 0.3)) { context in
            let talking = active && Int(context.date.timeIntervalSinceReferenceDate / 0.3) % 2 == 1
            Canvas { canvas, size in
                let cell = size.width / 8
                for run in sprite.runs(talking: talking) {
                    canvas.fill(
                        Path(CGRect(x: run.minX * cell, y: run.minY * cell, width: run.width * cell, height: cell)),
                        with: .color(color)
                    )
                }
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}
