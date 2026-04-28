"""Python SDK for the Reflow runtime.

Reflow is a modular flow-based programming runtime built on the actor
model. This package exposes a class-based authoring pattern on top of
the native bindings shipped as ``offbit_reflow._native``.

Quick start::

    from offbit_reflow import Actor, Network, Message

    class Doubler(Actor):
        component = "doubler"
        inports = ["in"]
        outports = ["out"]

        def run(self, ctx):
            n = ctx.inputs["in"]["data"]
            ctx.done({"out": Message.integer(n * 2)})

    net = Network()
    net.register_actor("tpl_doubler", Doubler())
    net.add_node("a", "tpl_doubler")
    net.add_initial("a", "in", {"type": "Integer", "data": 21})
    net.start()
"""

from __future__ import annotations

from typing import Any

from offbit_reflow import _native as _n

__version__ = _n.__version__

# ─── Re-exported native classes ────────────────────────────────────────────

Message = _n.Message
Stream = _n.Stream
StreamReader = _n.StreamReader
Graph = _n.Graph
SubgraphBuilder = _n.SubgraphBuilder
EventStream = _n.EventStream
ActorCallContext = _n.ActorCallContext
template_actor = _n.template_actor
template_list = _n.template_list
compose_graphs = _n.compose_graphs

# ─── Pack loader — for installing API services / GPU / ML / browser packs ──

load_pack = _n.load_pack
inspect_pack = _n.inspect_pack
list_packs = _n.list_packs
pack_abi_version = _n.pack_abi_version


class Actor:
    """Base class for Python-authored Reflow actors.

    Subclass and override :py:meth:`run`. Declare ports and await
    semantics as class-level attributes::

        class MyActor(Actor):
            component = "my_actor"
            inports = ["a", "b"]
            outports = ["sum"]
            await_all_inports = True

            def run(self, ctx):
                a = ctx.inputs["a"]["data"]
                b = ctx.inputs["b"]["data"]
                ctx.done({"sum": Message.integer(a + b)})

    Pass instances to ``Network.register_actor(template_id, instance)``.
    """

    component: str = "anonymous"
    inports: list[str] = []
    outports: list[str] = []
    await_all_inports: bool = False

    def run(self, ctx: ActorCallContext) -> None:  # type: ignore[name-defined]
        raise NotImplementedError(
            f"{type(self).__name__}.run() is not implemented"
        )

    def _build(self) -> Any:
        """Wrap this instance into a native ReflowActor. Internal.

        The native ``ActorCallContext.done`` / ``fail`` are themselves
        single-shot (a second call raises). We just wrap ``run`` so
        Python exceptions surface as ``ctx.fail(...)`` instead of
        unwinding into the runtime.
        """

        def callback(ctx: Any) -> None:
            try:
                self.run(ctx)
            except Exception as exc:  # noqa: BLE001 — surface as fail
                try:
                    ctx.fail(f"{type(exc).__name__}: {exc}")
                except Exception:
                    # Context is already resolved; nothing to do.
                    pass

        return _n.Actor.from_callback(
            component=type(self).component,
            inports=list(type(self).inports),
            outports=list(type(self).outports),
            callable=callback,
            await_all_inports=bool(type(self).await_all_inports),
        )


def _normalize_outputs(outputs: Any) -> Any:
    """Accept Message instances and convert to their JSON form so the
    native side can deserialize."""
    if outputs is None:
        return None
    if not isinstance(outputs, dict):
        raise TypeError(
            "ctx.done(outputs) expects a dict keyed by output port"
        )
    normalized: dict[str, Any] = {}
    for port, value in outputs.items():
        if value is None:
            continue
        if hasattr(value, "as_json"):
            normalized[port] = value.as_json()
        else:
            normalized[port] = value
    return normalized


# ─── Network — transparently wrap Actor instances ──────────────────────────


class Network(_n.Network):
    """``Network.register_actor`` accepts either a native ReflowActor or
    an :class:`Actor` class instance."""

    def register_actor(self, template_id: str, actor: Any) -> None:  # type: ignore[override]
        if isinstance(actor, Actor):
            actor = actor._build()
        super().register_actor(template_id, actor)


# Preserve the native constructor signature; Network inherits it.

_SubgraphBuilderNative = SubgraphBuilder


class SubgraphBuilder(_SubgraphBuilderNative):  # type: ignore[misc,valid-type]
    """``SubgraphBuilder.register_actor`` likewise accepts :class:`Actor`
    instances."""

    def register_actor(self, component_name: str, actor: Any) -> None:  # type: ignore[override]
        if isinstance(actor, Actor):
            actor = actor._build()
        super().register_actor(component_name, actor)


__all__ = [
    "__version__",
    "Actor",
    "ActorCallContext",
    "EventStream",
    "Graph",
    "Message",
    "Network",
    "Stream",
    "StreamReader",
    "SubgraphBuilder",
    "template_actor",
    "template_list",
    "compose_graphs",
]
