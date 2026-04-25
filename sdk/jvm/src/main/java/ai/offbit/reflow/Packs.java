package ai.offbit.reflow;

/**
 * Reflow actor-pack runtime API.
 *
 * Load `.rflpack` bundles (or raw cdylibs during development) at runtime
 * to extend the template catalog with additional actors — no rebuild of
 * the SDK required.
 */
public final class Packs {
    static { Reflow.ensureLoaded(); }

    private Packs() {}

    /**
     * Load a pack from either a {@code .rflpack} bundle or a raw cdylib
     * path. Returns the JSON array of template ids the pack published.
     * Idempotent per pack name: loading the same pack twice returns the
     * existing set without reloading.
     */
    public static String loadPack(String path) {
        return nativeLoadPack(path);
    }

    /**
     * Read the manifest of a {@code .rflpack} without loading its code.
     * Returns JSON matching {@link PackManifest}. Fails for raw dylibs
     * (they have no manifest).
     */
    public static String inspectPack(String path) {
        return nativeInspectPack(path);
    }

    /** Returns JSON describing every pack currently loaded into the process. */
    public static String listPacks() {
        return nativeListPacks();
    }

    /**
     * The pack ABI version this SDK was built against. Pack authors
     * must stamp the same value into their {@code .rflpack} manifests.
     */
    public static int packAbiVersion() {
        return nativePackAbiVersion();
    }

    private static native String nativeLoadPack(String path);
    private static native String nativeInspectPack(String path);
    private static native String nativeListPacks();
    private static native int nativePackAbiVersion();
}
