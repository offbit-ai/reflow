// Two-node pipeline: a Doubler actor emits 2×N into a Log actor.
//
// Build with the capi library on your link path:
//
//	cargo build -p reflow_rt_capi --release
//	cd sdk/go/examples/double_and_log
//	go run .
package main

import (
	"fmt"
	"time"

	reflow "github.com/offbit-ai/reflow/sdk/go"
)

type Doubler struct {
	reflow.BaseActor
}

func NewDoubler() *Doubler {
	return &Doubler{BaseActor: reflow.BaseActor{
		ComponentName: "doubler",
		InportsList:   []string{"in"},
		OutportsList:  []string{"out"},
	}}
}

func (d *Doubler) Run(ctx *reflow.ActorContext) error {
	in := ctx.Input("in")
	if in == nil {
		return nil
	}
	n, _ := in.AsInteger()
	return ctx.Emit("out", reflow.MessageInteger(n*2))
}

type Log struct {
	reflow.BaseActor
}

func NewLog() *Log {
	return &Log{BaseActor: reflow.BaseActor{
		ComponentName: "log",
		InportsList:   []string{"in"},
	}}
}

func (l *Log) Run(ctx *reflow.ActorContext) error {
	in := ctx.Input("in")
	if in == nil {
		return nil
	}
	raw, _ := in.AsJSON()
	fmt.Printf("got: %s\n", raw)
	return nil
}

func main() {
	net := reflow.NewNetwork()
	defer net.Close()

	if err := net.RegisterGoActor("tpl_doubler", NewDoubler()); err != nil {
		panic(err)
	}
	if err := net.RegisterGoActor("tpl_log", NewLog()); err != nil {
		panic(err)
	}

	_ = net.AddNode("a", "tpl_doubler", nil)
	_ = net.AddNode("b", "tpl_log", nil)
	_ = net.AddConnection("a", "out", "b", "in")
	_ = net.AddInitial("a", "in", map[string]any{"type": "Integer", "data": 21})

	if err := net.Start(); err != nil {
		panic(err)
	}
	time.Sleep(300 * time.Millisecond)
	_ = net.Shutdown()
}
