package reflow

import (
	"bytes"
	"testing"
	"time"
)

func TestMessageConstructorsAndAccessors(t *testing.T) {
	if got := MessageFlow().Kind(); got != KindFlow {
		t.Errorf("MessageFlow.Kind = %v, want Flow", got)
	}
	if v, ok := MessageBoolean(true).AsBoolean(); !ok || !v {
		t.Errorf("MessageBoolean(true).AsBoolean = (%v,%v)", v, ok)
	}
	if v, ok := MessageInteger(42).AsInteger(); !ok || v != 42 {
		t.Errorf("MessageInteger(42).AsInteger = (%d,%v)", v, ok)
	}
	if v, ok := MessageFloat(3.14).AsFloat(); !ok || v != 3.14 {
		t.Errorf("MessageFloat(3.14).AsFloat = (%v,%v)", v, ok)
	}
	if v, ok := MessageString("hi").AsString(); !ok || v != "hi" {
		t.Errorf("MessageString('hi').AsString = (%q,%v)", v, ok)
	}

	buf := []byte{1, 2, 3, 4}
	m := MessageBytes(buf)
	back, ok := m.AsBytes()
	if !ok || !bytes.Equal(back, buf) {
		t.Errorf("MessageBytes round-trip = (%v,%v)", back, ok)
	}
}

func TestMessageJSONRoundTrip(t *testing.T) {
	m := MessageInteger(7)
	raw, err := m.AsJSON()
	if err != nil {
		t.Fatalf("AsJSON: %v", err)
	}
	back, err := MessageFromJSON(raw)
	if err != nil {
		t.Fatalf("MessageFromJSON: %v", err)
	}
	if v, ok := back.AsInteger(); !ok || v != 7 {
		t.Errorf("round-trip integer = (%d,%v)", v, ok)
	}
}

func TestGraphAuthoring(t *testing.T) {
	g := NewGraph("demo", false)
	defer g.Close()

	if err := g.AddNode("a", "tpl_x", nil); err != nil {
		t.Fatalf("AddNode a: %v", err)
	}
	if err := g.AddNode("b", "tpl_y", map[string]any{"role": "sink"}); err != nil {
		t.Fatalf("AddNode b: %v", err)
	}
	if err := g.AddConnection("a", "out", "b", "in", nil); err != nil {
		t.Fatalf("AddConnection: %v", err)
	}
	if err := g.AddInitial("a", "in", map[string]any{"type": "Flow"}, nil); err != nil {
		t.Fatalf("AddInitial: %v", err)
	}

	raw, err := g.ToJSON()
	if err != nil {
		t.Fatalf("ToJSON: %v", err)
	}
	if !bytes.Contains(raw, []byte(`"tpl_x"`)) {
		t.Errorf("expected tpl_x in export, got %s", raw)
	}
}

// ─── Actor + Network pipeline ──────────────────────────────────────────────

type doubler struct {
	BaseActor
}

func newDoubler() *doubler {
	return &doubler{BaseActor: BaseActor{
		ComponentName: "doubler",
		InportsList:   []string{"in"},
		OutportsList:  []string{"out"},
	}}
}

func (d *doubler) Run(ctx *ActorContext) error {
	in := ctx.Input("in")
	if in == nil {
		return nil
	}
	n, _ := in.AsInteger()
	return ctx.Emit("out", MessageInteger(n*2))
}

type collect struct {
	BaseActor
	got chan int64
}

func newCollect() *collect {
	return &collect{
		BaseActor: BaseActor{
			ComponentName: "collect",
			InportsList:   []string{"in"},
		},
		got: make(chan int64, 4),
	}
}

func (c *collect) Run(ctx *ActorContext) error {
	in := ctx.Input("in")
	if in == nil {
		return nil
	}
	v, _ := in.AsInteger()
	select {
	case c.got <- v:
	default:
	}
	return nil
}

func TestNetworkPipeline(t *testing.T) {
	net := NewNetwork()
	defer net.Close()

	d := newDoubler()
	c := newCollect()

	if err := net.RegisterGoActor("tpl_doubler", d); err != nil {
		t.Fatalf("register doubler: %v", err)
	}
	if err := net.RegisterGoActor("tpl_collect", c); err != nil {
		t.Fatalf("register collect: %v", err)
	}

	if err := net.AddNode("a", "tpl_doubler", nil); err != nil {
		t.Fatalf("AddNode a: %v", err)
	}
	if err := net.AddNode("b", "tpl_collect", nil); err != nil {
		t.Fatalf("AddNode b: %v", err)
	}
	if err := net.AddConnection("a", "out", "b", "in"); err != nil {
		t.Fatalf("AddConnection: %v", err)
	}
	if err := net.AddInitial("a", "in", map[string]any{"type": "Integer", "data": 21}); err != nil {
		t.Fatalf("AddInitial: %v", err)
	}

	if err := net.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}

	select {
	case got := <-c.got:
		if got != 42 {
			t.Errorf("got %d, want 42", got)
		}
	case <-time.After(2 * time.Second):
		t.Fatalf("timed out waiting for collector to receive a value")
	}

	_ = net.Shutdown()
}

// ─── Streams ───────────────────────────────────────────────────────────────

func TestStreamProducerConsumer(t *testing.T) {
	s := NewStream(StreamOptions{BufferSize: 16, OriginActor: "a", OriginPort: "out"})
	for _, buf := range [][]byte{{1, 2, 3}, {9, 8, 7, 6}} {
		if err := s.SendBytes(buf); err != nil {
			t.Fatalf("SendBytes: %v", err)
		}
	}
	if err := s.End(); err != nil {
		t.Fatalf("End: %v", err)
	}

	msg := s.IntoMessage()
	if msg.Kind() != KindStreamHandle {
		t.Fatalf("into message kind = %v", msg.Kind())
	}
	rx, err := msg.TakeStream()
	if err != nil {
		t.Fatalf("TakeStream: %v", err)
	}
	defer rx.Close()

	var got [][]byte
	sawEnd := false
	for i := 0; i < 20 && !sawEnd; i++ {
		f, err := rx.Recv(250 * time.Millisecond)
		if err != nil {
			t.Fatalf("Recv: %v", err)
		}
		switch f.Kind {
		case FrameData:
			got = append(got, f.Data)
		case FrameEnd, FrameClosed:
			sawEnd = true
		case FrameTimeout:
			continue
		case FrameError:
			t.Fatalf("error frame: %s", f.Error)
		}
	}
	if !sawEnd {
		t.Fatalf("never saw end frame")
	}
	if len(got) != 2 || !bytes.Equal(got[0], []byte{1, 2, 3}) || !bytes.Equal(got[1], []byte{9, 8, 7, 6}) {
		t.Fatalf("unexpected frames: %v", got)
	}
}

// ─── Template catalog + subgraph ───────────────────────────────────────────

func TestTemplateCatalog(t *testing.T) {
	ids, err := TemplateList()
	if err != nil {
		t.Fatalf("TemplateList: %v", err)
	}
	if len(ids) < 100 {
		t.Errorf("catalog too small: %d", len(ids))
	}
	found := false
	for _, id := range ids {
		if id == "tpl_passthrough" {
			found = true
			break
		}
	}
	if !found {
		t.Errorf("expected tpl_passthrough in catalog")
	}

	h, err := TemplateActor("tpl_passthrough")
	if err != nil || h == nil {
		t.Fatalf("TemplateActor: %v", err)
	}
	h.Free()

	if _, err := TemplateActor("tpl_does_not_exist"); err == nil {
		t.Fatalf("unknown id must error")
	}
}

func TestSubgraphBuilder(t *testing.T) {
	export := []byte(`{
		"caseSensitive": false,
		"processes": { "t": {"id":"t","component":"my_tap","metadata":null} },
		"connections": [],
		"inports": {}, "outports": {},
		"properties": {"name": "sg"},
		"groups": []
	}`)

	type tap struct{ BaseActor }
	tapImpl := &tap{BaseActor: BaseActor{
		ComponentName: "my_tap",
		InportsList:   []string{"in"},
		OutportsList:  []string{"out"},
	}}
	_ = tapImpl

	b, err := NewSubgraphBuilder(export)
	if err != nil {
		t.Fatalf("NewSubgraphBuilder: %v", err)
	}

	if err := b.RegisterGoActor("my_tap", &noopActor{BaseActor: BaseActor{
		ComponentName: "my_tap",
		InportsList:   []string{"in"},
		OutportsList:  []string{"out"},
	}}); err != nil {
		t.Fatalf("RegisterGoActor: %v", err)
	}

	sg, err := b.Build()
	if err != nil {
		t.Fatalf("Build: %v", err)
	}

	net := NewNetwork()
	defer net.Close()
	if err := net.RegisterActor("tpl_sub", sg); err != nil {
		t.Fatalf("RegisterActor: %v", err)
	}
}

type noopActor struct{ BaseActor }

func (n *noopActor) Run(ctx *ActorContext) error { return nil }
