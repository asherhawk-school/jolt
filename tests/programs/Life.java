// Conway's Game of Life on a wrapping 24x12 board, starting with a glider gun seed.
public class Life {
    static final int W = 24, H = 12;

    static boolean[][] step(boolean[][] g) {
        boolean[][] next = new boolean[H][W];
        for (int y = 0; y < H; y++) {
            for (int x = 0; x < W; x++) {
                int n = 0;
                for (int dy = -1; dy <= 1; dy++)
                    for (int dx = -1; dx <= 1; dx++)
                        if ((dx != 0 || dy != 0) && g[(y + dy + H) % H][(x + dx + W) % W]) n++;
                next[y][x] = g[y][x] ? (n == 2 || n == 3) : n == 3;
            }
        }
        return next;
    }

    static void show(boolean[][] g, int gen) {
        int alive = 0;
        StringBuilder sb = new StringBuilder();
        for (boolean[] row : g) {
            for (boolean c : row) {
                sb.append(c ? '#' : '.');
                if (c) alive++;
            }
            sb.append('\n');
        }
        System.out.println("Generation " + gen + " (" + alive + " alive)");
        System.out.print(sb);
    }

    public static void main(String[] args) {
        boolean[][] g = new boolean[H][W];
        int[][] glider = {{0, 1}, {1, 2}, {2, 0}, {2, 1}, {2, 2}};
        for (int[] p : glider) g[p[0] + 1][p[1] + 1] = true;
        int[][] blinker = {{6, 15}, {6, 16}, {6, 17}};
        for (int[] p : blinker) g[p[0]][p[1]] = true;
        int[][] block = {{8, 4}, {8, 5}, {9, 4}, {9, 5}};
        for (int[] p : block) g[p[0]][p[1]] = true;
        for (int gen = 0; gen <= 12; gen++) {
            if (gen % 4 == 0) show(g, gen);
            g = step(g);
        }
    }
}
