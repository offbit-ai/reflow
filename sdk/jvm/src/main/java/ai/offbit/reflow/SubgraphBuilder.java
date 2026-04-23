package ai.offbit.reflow;

/**
 * Compose a {@code SubgraphActor} from a {@code GraphExport} JSON plus
 * an explicit actor map (falling back to the bundled catalog via
 * {@link #fillFromCatalog()}).
 */
public final class SubgraphBuilder implements AutoCloseable {
    static { Reflow.ensureLoaded(); }

    private long nativePtr;

    public SubgraphBuilder(String graphExportJson) {
        this.nativePtr = nativeNew(graphExportJson);
    }

    /** Register a user-supplied actor under {@code componentName}. */
    public void registerActor(String componentName, Actor actor) {
        nativeRegisterActor(nativePtr, componentName, actor.__build());
    }

    /** Register a bundled template under {@code componentName}. */
    public void registerActor(String componentName, long actorPtr) {
        nativeRegisterActor(nativePtr, componentName, actorPtr);
    }

    public void fillFromCatalog() {
        nativeFillFromCatalog(nativePtr);
    }

    /** Build the subgraph into an actor pointer. */
    public long build() {
        long p = nativeBuild(nativePtr);
        nativePtr = 0;
        return p;
    }

    @Override
    public void close() {
        if (nativePtr != 0) {
            nativeFree(nativePtr);
            nativePtr = 0;
        }
    }

    @Override
    @SuppressWarnings("deprecation")
    protected void finalize() {
        close();
    }

    private static native long nativeNew(String json);
    private static native void nativeRegisterActor(long ptr, String component, long actorPtr);
    private static native void nativeFillFromCatalog(long ptr);
    private static native long nativeBuild(long ptr);
    private static native void nativeFree(long ptr);
}
