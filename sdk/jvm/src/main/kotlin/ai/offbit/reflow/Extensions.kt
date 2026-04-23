@file:JvmName("ReflowExtensions")

package ai.offbit.reflow

/**
 * Register a Kotlin [Actor] under a template id.
 *
 * (The Java overload `registerActor(String, Actor)` already handles
 * this; this extension exists so code using the [actor] DSL can stay
 * consistent when the actor is assigned to a `val`.)
 */
fun Network.register(templateId: String, actor: Actor) {
    registerActor(templateId, actor)
}

/** Destructuring sugar for [StreamReader.Frame]. */
operator fun StreamReader.Frame.component1(): StreamReader.FrameKind = kind
operator fun StreamReader.Frame.component2(): ByteArray? = data
operator fun StreamReader.Frame.component3(): String? = error

/**
 * Build a Subgraph with the Kotlin [actor] DSL inline:
 *
 * ```kotlin
 * val sg = subgraph(exportJson) {
 *     registerActor("my_custom", actor { ... })
 *     fillFromCatalog()
 * }.build()
 * ```
 */
inline fun subgraph(
    graphExportJson: String,
    block: SubgraphBuilder.() -> Unit,
): SubgraphBuilder = SubgraphBuilder(graphExportJson).apply(block)
