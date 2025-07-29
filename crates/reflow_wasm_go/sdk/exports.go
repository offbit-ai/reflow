// Package reflow provides export templates for WASM plugins
package reflow

// ExportTemplate provides the template code that needs to be added to main.go
// This is necessary because //export directives only work in the main package
const ExportTemplate = `
//export get_metadata
func get_metadata() int32 {
	return reflow.GetMetadata()
}

//export process
func process() int32 {
	return reflow.Process()
}
`