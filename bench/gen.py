#!/usr/bin/env python3
"""Generate a large synthetic Java project that both javac and jolt accept.

usage: gen.py OUT_DIR NUM_FILES METHODS_PER_FILE
"""
import os
import sys

out, nfiles, nmethods = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
os.makedirs(out, exist_ok=True)

METHOD = '''
    static long work{m}(int n) {{
        long acc = {m};
        int[] data = new int[n + 8];
        for (int i = 0; i < data.length; i++) {{
            data[i] = (i * 31 + {m}) % 97;
        }}
        for (int i = 0; i < n; i++) {{
            if (data[i] % 3 == 0) {{
                acc += data[i] * 2L;
            }} else if (data[i] % 5 == 1) {{
                acc -= data[i] >> 1;
            }} else {{
                acc ^= i;
            }}
            switch (i & 3) {{
                case 0: acc += 7; break;
                case 1: acc -= 3; break;
                default: acc *= 1;
            }}
        }}
        String s = "m{m}:" + acc + ":" + n;
        StringBuilder sb = new StringBuilder(s);
        while (sb.length() < 24) sb.append('#');
        double d = Math.sqrt(acc < 0 ? -acc : acc) + s.length() / 2.0;
        Point{f} p = new Point{f}(n, {m});
        p.move(3, -2);
        return acc + (long) d + sb.length() + p.dist2() + helper{f}(n, {m});
    }}
'''

for f in range(nfiles):
    body = []
    body.append(f'''class Point{f} {{
    int x, y;
    Point{f}(int x, int y) {{ this.x = x; this.y = y; }}
    void move(int dx, int dy) {{ x += dx; y += dy; }}
    long dist2() {{ return (long) x * x + (long) y * y; }}
    @Override
    public String toString() {{ return "(" + x + ", " + y + ")"; }}
}}

public class Mod{f} {{
    static final int ID = {f};
    static int helper{f}(int a, int b) {{
        int r = 0;
        for (int i = 0; i < a; i++) r += (i ^ b) & 15;
        return r;
    }}
''')
    for m in range(nmethods):
        body.append(METHOD.format(f=f, m=m))
    calls = " + ".join(f"work{m}(n)" for m in range(min(nmethods, 8)))
    body.append(f'''
    static long run(int n) {{
        return {calls};
    }}
}}
''')
    with open(os.path.join(out, f"Mod{f}.java"), "w") as fh:
        fh.write("".join(body))

with open(os.path.join(out, "Main.java"), "w") as fh:
    lines = "\n".join(f"        total += Mod{f}.run(20);" for f in range(nfiles))
    fh.write(f'''public class Main {{
    public static void main(String[] args) {{
        long total = 0;
{lines}
        System.out.println("total = " + total);
    }}
}}
''')
