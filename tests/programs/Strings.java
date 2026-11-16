import java.util.Arrays;

public class Strings {
    static String reverse(String s) {
        StringBuilder sb = new StringBuilder(s);
        return sb.reverse().toString();
    }

    static boolean isPalindrome(String s) {
        int i = 0, j = s.length() - 1;
        while (i < j) {
            if (s.charAt(i++) != s.charAt(j--)) return false;
        }
        return true;
    }

    static String caesar(String s, int k) {
        char[] cs = s.toCharArray();
        for (int i = 0; i < cs.length; i++) {
            char c = cs[i];
            if (Character.isUpperCase(c)) cs[i] = (char) ('A' + (c - 'A' + k) % 26);
            else if (Character.isLowerCase(c)) cs[i] = (char) ('a' + (c - 'a' + k) % 26);
        }
        return new String(cs);
    }

    static int countVowels(String s) {
        int n = 0;
        for (char c : s.toLowerCase().toCharArray()) {
            if ("aeiou".indexOf(c) >= 0) n++;
        }
        return n;
    }

    public static void main(String[] args) {
        String s = "Hello, World";
        System.out.println(s.length() + " " + s.charAt(4) + " " + s.indexOf("World") + " " + s.substring(7));
        System.out.println(s.toUpperCase() + s.toLowerCase());
        System.out.println(reverse(s));
        System.out.println(isPalindrome("racecar") + " " + isPalindrome("abc"));
        System.out.println(caesar("Attack at Dawn!", 3));
        System.out.println(countVowels("Programming Language"));
        System.out.println(String.join("-", "a", "b", "c"));
        String[] parts = "one,two,,three".split(",");
        System.out.println(parts.length + " " + Arrays.toString(parts));
        System.out.println("  trim me  ".trim() + "|");
        System.out.println("abc".compareTo("abd") + " " + "b".compareTo("a"));
        System.out.println("ab".repeat(3));
        System.out.println(String.valueOf(3.5) + String.valueOf(true) + String.valueOf('z') + String.valueOf(12L));
        System.out.println("x".equals("x") + " " + "X".equalsIgnoreCase("x") + " " + "abc".contains("bc"));
        System.out.println("a-b-c".replace('-', '+') + " " + "aaa".replace("a", "bb"));

        String acc = "";
        for (int i = 0; i < 5; i++) acc += i;
        System.out.println(acc);
        acc += 'X';
        acc += 1.5;
        acc += true;
        acc += null;
        System.out.println(acc);
        String[] arr = {"p", "q"};
        arr[0] += "!" + 2;
        System.out.println(arr[0] + arr[1]);

        StringBuilder sb = new StringBuilder();
        for (char c = 'a'; c <= 'e'; c++) sb.append(c).append(',');
        sb.setLength(sb.length() - 1);
        sb.insert(0, "[").append("]");
        System.out.println(sb);
        sb.setCharAt(1, 'A');
        sb.deleteCharAt(2);
        System.out.println(sb.toString() + " len=" + sb.length());

        System.out.println(String.format("%5d|%-5s|%08.3f|%x|%c|%b", 42, "ab", 3.14159, 255, 'q', false));
        System.out.printf("%s has %d chars%n", s, s.length());
        System.out.printf("%.1f%%%n", 12.345);
        long big = 1234567890123L;
        System.out.println(String.format("%,d", big));

        int x = 7;
        double y = 2.5;
        char c = 'k';
        boolean b = false;
        long z = -5;
        int[] none = null;
        Object obj = null;
        System.out.println("x=" + x + " y=" + y + " c=" + c + " b=" + b + " z=" + z + " none=" + none + " obj=" + obj);
        System.out.println(x + y + "!" + x + y);
        System.out.println(c + x + "" + c + x);
        char[] hello = {'h', 'i'};
        System.out.println(hello);
        System.out.println(String.valueOf(hello) + "!");
        System.out.println(Integer.toString(255, 16) + " " + Long.toBinaryString(5L) + " " + Character.toUpperCase('m'));
        System.out.println(Character.isDigit('7') + " " + Character.isLetter('7') + " " + Character.getNumericValue('7'));
        String multi = "line1\nline2\ttabbed \"quoted\" \\ A";
        System.out.println(multi);
        System.out.println("unicode: é中 " + "é" + "中".length());
    }
}
