package ai.offbit.reflow;

/**
 * Static bootstrap helper for the Reflow native library.
 *
 * <p>Loaded on first access to any SDK class. Looks up the library path
 * from the {@code reflow.native.lib} system property (absolute path) or
 * from {@code java.library.path} via {@code System.loadLibrary}.
 */
final class Reflow {
    private Reflow() {}

    private static boolean loaded = false;

    static synchronized void ensureLoaded() {
        if (loaded) return;
        String path = System.getProperty("reflow.native.lib");
        if (path != null && !path.isEmpty()) {
            System.load(path);
        } else {
            System.loadLibrary("reflow_rt_jvm");
        }
        loaded = true;
    }
}
