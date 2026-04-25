package ai.offbit.reflow;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;

/**
 * Static bootstrap helper for the Reflow native library.
 *
 * <p>Resolution order (first that wins):
 *
 * <ol>
 *   <li>{@code -Dreflow.native.lib=<absolute path>} — explicit
 *       override, used by tests and unusual deployments.</li>
 *   <li>Classpath resource at {@code /native/<host>/<libname>} — the
 *       Maven Central jar bundles the library for every supported
 *       triple. We extract the right one to {@code java.io.tmpdir}
 *       and {@link System#load(String)} it.</li>
 *   <li>{@link System#loadLibrary(String) System.loadLibrary("reflow_rt_jvm")}
 *       — repo-local dev path (gradle test sets {@code java.library.path}
 *       to {@code sdk/jvm/src/native/target/<profile>}).</li>
 * </ol>
 */
final class Reflow {
    private Reflow() {}

    private static boolean loaded = false;

    static synchronized void ensureLoaded() {
        if (loaded) return;

        String override = System.getProperty("reflow.native.lib");
        if (override != null && !override.isEmpty()) {
            System.load(override);
            loaded = true;
            return;
        }

        if (loadFromClasspath()) {
            loaded = true;
            return;
        }

        // Fallback for repo-local dev: java.library.path-based lookup.
        System.loadLibrary("reflow_rt_jvm");
        loaded = true;
    }

    /**
     * Try to extract the platform-specific library from a classpath
     * resource into a temp file and load it. Returns false if no
     * matching resource is on the classpath (e.g. dev jars without
     * the bundled libs).
     */
    private static boolean loadFromClasspath() {
        String dir = hostResourceDir();
        String lib = hostLibName();
        if (dir == null || lib == null) return false;

        String resource = "/native/" + dir + "/" + lib;
        try (InputStream in = Reflow.class.getResourceAsStream(resource)) {
            if (in == null) return false;
            String suffix = lib.substring(lib.lastIndexOf('.'));
            File tmp = File.createTempFile("libreflow_rt_jvm-", suffix);
            tmp.deleteOnExit();
            try (FileOutputStream out = new FileOutputStream(tmp)) {
                in.transferTo(out);
            }
            System.load(tmp.getAbsolutePath());
            return true;
        } catch (IOException e) {
            throw new UnsatisfiedLinkError("extracting " + resource + ": " + e.getMessage());
        }
    }

    private static String hostResourceDir() {
        String os = System.getProperty("os.name", "").toLowerCase();
        String arch = System.getProperty("os.arch", "").toLowerCase();
        boolean arm64 = arch.contains("aarch64") || arch.contains("arm64");
        if (os.contains("mac"))   return arm64 ? "darwin-arm64"  : "darwin-x86_64";
        if (os.contains("nux"))   return arm64 ? "linux-aarch64" : "linux-x86_64";
        if (os.contains("win"))   return "windows-x86_64";
        return null;
    }

    private static String hostLibName() {
        String os = System.getProperty("os.name", "").toLowerCase();
        if (os.contains("mac"))   return "libreflow_rt_jvm.dylib";
        if (os.contains("nux"))   return "libreflow_rt_jvm.so";
        if (os.contains("win"))   return "reflow_rt_jvm.dll";
        return null;
    }
}
