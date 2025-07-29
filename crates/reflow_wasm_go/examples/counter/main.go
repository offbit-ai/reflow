package main

import (
	reflow "github.com/darmie/reflow/reflow_wasm_go/sdk"
)

func processCounter(context reflow.ActorContext) (reflow.ActorResult, error) {
	// processCounter called
	outputs := make(map[string]reflow.Message)
	
	// Get current count from state (passed in context)
	var count int64 = 0
	if context.State != nil {
		// Try to extract count from state
		if stateMap, ok := context.State.(map[string]interface{}); ok {
			if countVal, exists := stateMap["count"]; exists {
				if countFloat, ok := countVal.(float64); ok {
					count = int64(countFloat)
				}
			}
		}
	}
	// Current count from state
	
	// Handle different operations if provided
	if operation, exists := context.Payload["operation"]; exists {
		// Got operation
		if operation.Type == "String" {
			op := operation.Data.(string)
			// Operation is: op
			
			switch op {
			case "increment":
				count++
			case "decrement":
				count--
			case "reset":
				count = 0
			default:
				// No change for unknown operations
			}
			
			// New count after operation
			
			// Return current value and operation
			outputs["value"] = reflow.NewIntegerMessage(count)
			outputs["operation"] = reflow.NewStringMessage(op)
			
		} else {
			// Operation type is not string
			outputs["value"] = reflow.NewIntegerMessage(count)
		}
	} else {
		// No operation specified, just return current count
		// No operation specified, returning current count
		outputs["value"] = reflow.NewIntegerMessage(count)
	}
	
	// Update state with new count
	newState := map[string]interface{}{
		"count": count,
	}
	
	// Convert Message outputs to interface{} for ActorResult
	plainOutputs := make(map[string]interface{})
	for k, msg := range outputs {
		plainOutputs[k] = msg.ToSerializable()
	}
	
	result := reflow.ActorResult{
		Outputs: plainOutputs,
		State:   newState,
	}
	
	// Returning result
	
	// Result prepared
	
	return result, nil
}

func getCounterMetadata() reflow.PluginMetadata {
	return reflow.PluginMetadata{
		Component:   "GoCounter",
		Description: "Counter actor implemented in Go (testing version)",
		Inports: []reflow.PortDefinition{
			reflow.NewOptionalPort("operation", "Operation to perform (increment, decrement, reset)", "String"),
		},
		Outports: []reflow.PortDefinition{
			reflow.NewOptionalPort("value", "Current counter value", "Integer"),
			reflow.NewOptionalPort("operation", "Operation that was performed", "String"),
		},
		ConfigSchema: nil,
	}
}

func init() {
	// Go counter init
	reflow.RegisterPlugin(getCounterMetadata(), processCounter)
}

// Export functions required by Extism
// Use //export for standard Go, //go:wasmexport for TinyGo

//export get_metadata
func get_metadata() int32 {
	// get_metadata called
	return reflow.GetMetadata()
}

//export process
func process() int32 {
	// process called
	return reflow.Process()
}

func main() {}