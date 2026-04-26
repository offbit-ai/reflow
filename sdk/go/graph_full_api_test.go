package reflow

import (
	"encoding/json"
	"testing"
)

func TestGraphFullAPI_RenameNode(t *testing.T) {
	g := NewGraph("rename", false)
	defer g.Close()
	mustNoErr(t, g.AddNode("a", "tpl_x", nil))
	mustNoErr(t, g.AddNode("b", "tpl_y", nil))
	mustNoErr(t, g.AddConnection("a", "out", "b", "in", nil))

	mustNoErr(t, g.RenameNode("a", "alpha"))
	conn, err := g.GetConnectionJSON("alpha", "out", "b", "in")
	mustNoErr(t, err)
	if conn == nil {
		t.Fatalf("expected connection under alpha->b after rename")
	}
}

func TestGraphFullAPI_GroupsCRUD(t *testing.T) {
	g := NewGraph("groups", false)
	defer g.Close()
	for _, id := range []string{"a", "b", "c"} {
		mustNoErr(t, g.AddNode(id, "tpl_x", nil))
	}
	mustNoErr(t, g.AddGroup("g1", []string{"a", "b"}, map[string]any{"tag": "left"}))
	mustNoErr(t, g.AddToGroup("g1", "c"))
	mustNoErr(t, g.RemoveFromGroup("g1", "a"))
	mustNoErr(t, g.SetGroupMetadata("g1", map[string]any{"tag": "right"}))

	rawGroups, err := g.GroupsJSON()
	mustNoErr(t, err)
	var groups []struct {
		ID       string         `json:"id"`
		Nodes    []string       `json:"nodes"`
		Metadata map[string]any `json:"metadata"`
	}
	mustNoErr(t, json.Unmarshal(rawGroups, &groups))
	if len(groups) != 1 || groups[0].ID != "g1" {
		t.Fatalf("groups = %+v", groups)
	}
	if !setEq(groups[0].Nodes, []string{"b", "c"}) {
		t.Fatalf("group nodes = %+v, want [b c]", groups[0].Nodes)
	}
	if groups[0].Metadata["tag"] != "right" {
		t.Fatalf("group metadata = %+v", groups[0].Metadata)
	}

	mustNoErr(t, g.RemoveGroup("g1"))
	rawGroups, _ = g.GroupsJSON()
	if string(rawGroups) != "[]" {
		t.Fatalf("expected empty groups after remove, got %s", rawGroups)
	}
}

func TestGraphFullAPI_PortsAndMetadata(t *testing.T) {
	g := NewGraph("ports", false)
	defer g.Close()
	mustNoErr(t, g.AddNode("a", "tpl_x", nil))
	mustNoErr(t, g.AddInport("input", "a", "in", map[string]any{"type": "flow"}, nil))
	mustNoErr(t, g.AddOutport("output", "a", "out", map[string]any{"type": "flow"}, nil))

	mustNoErr(t, g.RenameInport("input", "left"))
	mustNoErr(t, g.RenameOutport("output", "right"))
	mustNoErr(t, g.SetInportMetadata("left", map[string]any{"caption": "L"}))
	mustNoErr(t, g.SetOutportMetadata("right", map[string]any{"caption": "R"}))

	rawIn, _ := g.InportsJSON()
	rawOut, _ := g.OutportsJSON()
	if !contains(rawIn, "left") || !contains(rawOut, "right") {
		t.Fatalf("rename did not propagate: in=%s out=%s", rawIn, rawOut)
	}

	mustNoErr(t, g.RemoveInport("left"))
	mustNoErr(t, g.RemoveOutport("right"))
}

func TestGraphFullAPI_ConnectionAndInitialRemoval(t *testing.T) {
	g := NewGraph("conn", false)
	defer g.Close()
	mustNoErr(t, g.AddNode("a", "tpl_x", nil))
	mustNoErr(t, g.AddNode("b", "tpl_y", nil))
	mustNoErr(t, g.AddConnection("a", "out", "b", "in", nil))
	mustNoErr(t, g.SetConnectionMetadata("a", "out", "b", "in", map[string]any{"weight": 1}))
	mustNoErr(t, g.AddInitial("a", "in", map[string]any{"type": "Integer", "data": 42}, nil))

	conns, _ := g.ConnectionsJSON()
	inits, _ := g.InitializersJSON()
	if !contains(conns, "weight") {
		t.Fatalf("connection metadata not present: %s", conns)
	}
	if !contains(inits, "Integer") {
		t.Fatalf("initial not present: %s", inits)
	}

	mustNoErr(t, g.RemoveConnection("a", "out", "b", "in"))
	mustNoErr(t, g.RemoveInitial("a", "in"))
	conns, _ = g.ConnectionsJSON()
	inits, _ = g.InitializersJSON()
	if string(conns) != "[]" {
		t.Fatalf("expected no connections, got %s", conns)
	}
	if string(inits) != "[]" {
		t.Fatalf("expected no initializers, got %s", inits)
	}
}

func TestGraphFullAPI_PropertiesAndImport(t *testing.T) {
	g := NewGraph("props", false)
	defer g.Close()
	mustNoErr(t, g.SetProperties(map[string]any{"author": "darmie"}))
	rawProps, _ := g.PropertiesJSON()
	if !contains(rawProps, "darmie") {
		t.Fatalf("props missing: %s", rawProps)
	}

	seed := NewGraph("seed", false)
	defer seed.Close()
	mustNoErr(t, seed.AddNode("x", "tpl_x", nil))
	rawSeed, _ := seed.ToJSON()
	mustNoErr(t, g.Import(rawSeed))
	got, _ := g.GetNodeJSON("x")
	if got == nil {
		t.Fatalf("expected node x after import")
	}
}

// ─── helpers ───────────────────────────────────────────────────────────────

func mustNoErr(t *testing.T, err error) {
	t.Helper()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func contains(haystack []byte, needle string) bool {
	return len(haystack) > 0 && len(needle) > 0 && bytesIndex(haystack, []byte(needle)) >= 0
}

func bytesIndex(haystack, needle []byte) int {
outer:
	for i := 0; i+len(needle) <= len(haystack); i++ {
		for j, c := range needle {
			if haystack[i+j] != c {
				continue outer
			}
		}
		return i
	}
	return -1
}

func setEq(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	m := map[string]int{}
	for _, x := range a {
		m[x]++
	}
	for _, y := range b {
		m[y]--
	}
	for _, n := range m {
		if n != 0 {
			return false
		}
	}
	return true
}
