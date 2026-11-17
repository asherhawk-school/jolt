// The Mandelbrot set, drawn with characters.
// About 300,000 loop iterations of double-precision math.
public class Mandelbrot {
    public static void main(String[] args) {
        int width = 76, height = 30, maxIter = 250;
        String shades = " .,:;-=+*#%@";
        StringBuilder out = new StringBuilder();
        for (int row = 0; row < height; row++) {
            double ci = -1.25 + row * 2.5 / height;
            for (int col = 0; col < width; col++) {
                double cr = -2.25 + col * 3.1 / width;
                double zr = 0, zi = 0;
                int i = 0;
                while (i < maxIter && zr * zr + zi * zi <= 4.0) {
                    double t = zr * zr - zi * zi + cr;
                    zi = 2 * zr * zi + ci;
                    zr = t;
                    i++;
                }
                out.append(i == maxIter ? '@' : shades.charAt(i % (shades.length() - 1)));
            }
            out.append('\n');
        }
        System.out.print(out);
    }
}
