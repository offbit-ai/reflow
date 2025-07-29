package main

import (
	reflow "github.com/darmie/reflow/reflow_wasm_go/sdk"
)

func processCounterWithState(context reflow.ActorContext) (reflow.ActorResult, error) {
	// processCounterWithState called
	outputs := make(map[string]reflow.Message)
	
	// Get current count from state using host function
	currentCount := int64(0)
	if stateValue, err := reflow.GetState("count"); err == nil && stateValue != nil {
		if countFloat, ok := stateValue.(float64); ok {
			currentCount = int64(countFloat)
		}
		// Retrieved count from state via host function
	} else {
		// No count in state, starting from 0
	}
	
	// Handle different operations if provided
	if operation, exists := context.Payload["operation"]; exists {
		// Got operation
		if operation.Type == "String" {
			op := operation.Data.(string)
			// Operation is: op
			
			switch op {
			case "increment":
				currentCount++
			case "decrement":
				currentCount--
			case "reset":
				currentCount = 0
			case "double":
				currentCount *= 2
			default:
				// No change for unknown operations
			}
			
			// New count after operation
			
			// Save new count to state using host function
			// Save new count to state using host function
			reflow.SetState("count", currentCount)
			
			// Return current value and operation
			outputs["value"] = reflow.NewIntegerMessage(currentCount)
			outputs["operation"] = reflow.NewStringMessage(op)
			
			// Also include an event about the change
			oldValue := currentCount
			switch op {
			case "increment":
				oldValue = currentCount - 1
			case "decrement":
				oldValue = currentCount + 1
			case "double":
				oldValue = currentCount / 2
			case "reset":
				oldValue = 0
			}
			
			eventData := map[string]interface{}{
				"old_value": oldValue,
				"new_value": currentCount,
				"operation": op,
			}
			outputs["count_changed"] = reflow.NewEventMessage(eventData)
			
			// Demonstrate async output using host function
			asyncOutputs := map[string]interface{}{
				"status": "Processing complete",
				"count": currentCount,
			}
			// Send async output via host function
			reflow.SendOutput(asyncOutputs)
			
		} else {
			// Operation type is not string
			outputs["value"] = reflow.NewIntegerMessage(currentCount)
		}
	} else {
		// No operation specified, just return current count
		// No operation specified, returning current count
		outputs["value"] = reflow.NewIntegerMessage(currentCount)
	}
	
	// Update state with new count
	newState := map[string]interface{}{
		"count": currentCount,
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

func getCounterWithStateMetadata() reflow.PluginMetadata {
	return reflow.PluginMetadata{
		Component:   "GoCounterWithState",
		Description: "Stateful counter actor implemented in Go with host functions",
		Inports: []reflow.PortDefinition{
			reflow.NewOptionalPort("operation", "Operation to perform (increment, decrement, reset, double)", "String"),
		},
		Outports: []reflow.PortDefinition{
			reflow.NewOptionalPort("value", "Current counter value", "Integer"),
			reflow.NewOptionalPort("operation", "Operation that was performed", "String"),
			reflow.NewOptionalPort("count_changed", "Event when count changes", "Event"),
		},
		ConfigSchema: nil,
	}
}

func init() {
	// Go counter with state init
	reflow.RegisterPlugin(getCounterWithStateMetadata(), processCounterWithState)
}

// Export functions required by Extism
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