public class Switches {
    static String day(int d) {
        switch (d) {
            case 1: return "Mon";
            case 2: return "Tue";
            case 3: return "Wed";
            case 4: return "Thu";
            case 5: return "Fri";
            case 6:
            case 7:
                return "Weekend";
            default:
                return "???";
        }
    }

    static int sparse(int v) {
        switch (v) {
            case -1000: return 1;
            case 7: return 2;
            case 100000: return 3;
            case Integer.MAX_VALUE: return 4;
            default: return 0;
        }
    }

    static String arrow(char c) {
        String r = "";
        switch (c) {
            case 'a', 'e', 'i', 'o', 'u' -> r = "vowel";
            case ' ' -> r = "space";
            default -> {
                if (Character.isDigit(c)) r = "digit";
                else r = "consonant";
            }
        }
        return r;
    }

    static int command(String s) {
        switch (s) {
            case "start": return 1;
            case "stop": return 2;
            case "pause":
            case "wait":
                return 3;
            default: return -1;
        }
    }

    public static void main(String[] args) {
        for (int i = 0; i <= 8; i++) System.out.print(day(i) + " ");
        System.out.println();
        System.out.println(sparse(-1000) + " " + sparse(7) + " " + sparse(100000) + " " + sparse(Integer.MAX_VALUE) + " " + sparse(8));
        for (char c : "hi 2u".toCharArray()) System.out.print(arrow(c) + " ");
        System.out.println();
        System.out.println(command("start") + " " + command("stop") + " " + command("wait") + " " + command("pause") + " " + command("go"));

        int fall = 0;
        switch (2) {
            case 1: fall += 1;
            case 2: fall += 2;
            case 3: fall += 3;
                break;
            case 4: fall += 4;
        }
        System.out.println(fall);

        int count = 0;
        for (int i = 0; i < 10; i++) {
            switch (i % 3) {
                case 0:
                    continue;
                case 1:
                    count += 10;
                    break;
                default:
                    count++;
            }
        }
        System.out.println(count);

        String kind = "x";
        switch (kind) {
            case "y" -> System.out.println("y!");
            default -> System.out.println("default!");
        }
        switch (5) {
        }
        switch (3) {
            default: System.out.println("only default");
        }
    }
}
