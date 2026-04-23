@file:JvmName("ReflowCoroutines")

package ai.offbit.reflow

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.withContext

/**
 * Suspend-friendly adapters over the blocking JNI `recv` calls. The
 * underlying native API blocks a thread — these wrappers move the block
 * onto [Dispatchers.IO] so coroutines stay cooperative.
 */

/** Suspend for up to [timeoutMs] ms for the next network event. */
suspend fun EventStream.recvAsync(timeoutMs: Int = 200): String? =
    withContext(Dispatchers.IO) { recv(timeoutMs) }

/**
 * Cold [Flow] of network events. Drains until [predicate] returns false
 * (default: forever). Poll timeouts are silently skipped.
 */
fun EventStream.asFlow(pollMs: Int = 200): Flow<String> = flow {
    while (true) {
        val evt = withContext(Dispatchers.IO) { recv(pollMs) } ?: continue
        emit(evt)
    }
}

/** Suspend for up to [timeoutMs] ms for the next stream frame. */
suspend fun StreamReader.recvAsync(timeoutMs: Int = 500): StreamReader.Frame =
    withContext(Dispatchers.IO) { recv(timeoutMs) }

/**
 * Cold [Flow] of stream frames. Terminates on END / ERROR / CLOSED.
 * TIMEOUT and BEGIN frames are emitted as-is so callers can distinguish.
 */
fun StreamReader.asFlow(timeoutMs: Int = 500): Flow<StreamReader.Frame> = flow {
    while (true) {
        val f = withContext(Dispatchers.IO) { recv(timeoutMs) }
        emit(f)
        when (f.kind) {
            StreamReader.FrameKind.END,
            StreamReader.FrameKind.ERROR,
            StreamReader.FrameKind.CLOSED -> return@flow
            else -> {} // keep iterating
        }
    }
}
