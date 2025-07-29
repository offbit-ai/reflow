package main

import (
	"encoding/json"
	"github.com/extism/go-pdk"
)

//go:wasmexport get_metadata
func get_metadata() int32 {
	// Test outputting metadata
	metadata := map[string]interface{}{
		"component":   "DebugTest",
		"description": "Debug test plugin",
		"inports":     []interface{}{},
		"outports":    []interface{}{},
	}
	
	data, err := json.Marshal(metadata)
	if err != nil {
		pdk.SetError(err)
		return 1
	}
	
	// Use pdk.Output to send raw bytes
	pdk.Output(data)
	return 0
}

//go:wasmexport process
func process() int32 {
	// Get input
	_ = pdk.Input()
	
	// Create simple result
	result := map[string]interface{}{
		"outputs": map[string]interface{}{
			"test": map[string]interface{}{
				"type": "String",
				"data": "Hello from Go!",
			},
		},
	}
	
	data, err := json.Marshal(result)
	if err != nil {
		pdk.SetError(err)
		return 1
	}
	
	// Output the result
	pdk.Output(data)
	return 0
}

func main() {}