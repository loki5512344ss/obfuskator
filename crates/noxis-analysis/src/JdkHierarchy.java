import java.util.Scanner;

/**
 * Answers superclass queries from the obfuscator's frame analysis.
 *
 * <p>Protocol: one internal class name per line on stdin
 * ({@code java/io/BufferedReader}); one line back on stdout - the direct
 * superclass as an internal name, or an empty line when unknown
 * (missing dependency, interface, primitive, array). A single process
 * serves the whole analysis run; EOF on stdin ends it.
 */
public class JdkHierarchy {
    public static void main(String[] args) {
        Scanner in = new Scanner(System.in);
        while (in.hasNextLine()) {
            String name = in.nextLine().trim().replace('/', '.');
            String sup = "";
            if (!name.isEmpty()) {
                try {
                    Class<?> loaded =
                            Class.forName(name, false, JdkHierarchy.class.getClassLoader());
                    Class<?> parent = loaded.getSuperclass();
                    if (parent != null) {
                        sup = parent.getName().replace('.', '/');
                    }
                } catch (Throwable ignored) {
                    // Unknown: not on the JDK classpath, an interface with
                    // no superclass, a primitive or an array - empty line.
                }
            }
            System.out.println(sup);
        }
    }
}
