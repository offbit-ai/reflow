package ai.offbit.reflow;

/** Bundled {@code reflow_components} template catalog. */
public final class Templates {
    static { Reflow.ensureLoaded(); }

    private Templates() {}

    /**
     * Instantiate a bundled template as a native actor pointer. Hand to
     * {@link Network#registerActor(String, long)}.
     */
    public static long templateActor(String templateId) {
        long ptr = nativeTemplateActor(templateId);
        if (ptr == 0) {
            throw new IllegalArgumentException("unknown template id: " + templateId);
        }
        return ptr;
    }

    /** Returns the JSON array of every template id in the catalog. */
    public static String templateListJson() {
        return nativeTemplateList();
    }

    private static native long nativeTemplateActor(String id);
    private static native String nativeTemplateList();
}
