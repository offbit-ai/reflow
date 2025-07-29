// Package reflow provides the Go SDK for building Reflow actors as WebAssembly plugins
package reflow

import (
	"encoding/json"
	"fmt"

	"github.com/extism/go-pdk"
)

// Message represents different types of data that can be passed between actors
type Message struct {
	Type string      `json:"type"`
	Data interface{} `json:"data"`
}

// NewFlowMessage creates a flow control message
func NewFlowMessage() Message {
	return Message{Type: "Flow", Data: nil}
}

// NewEventMessage creates an event message with JSON data
func NewEventMessage(data interface{}) Message {
	return Message{Type: "Event", Data: data}
}

// NewBooleanMessage creates a boolean message
func NewBooleanMessage(value bool) Message {
	return Message{Type: "Boolean", Data: value}
}

// NewIntegerMessage creates an integer message
func NewIntegerMessage(value int64) Message {
	return Message{Type: "Integer", Data: value}
}

// NewFloatMessage creates a float message
func NewFloatMessage(value float64) Message {
	return Message{Type: "Float", Data: value}
}

// NewStringMessage creates a string message
func NewStringMessage(value string) Message {
	return Message{Type: "String", Data: value}
}

// NewObjectMessage creates an object message with JSON data
func NewObjectMessage(data interface{}) Message {
	return Message{Type: "Object", Data: data}
}

// NewArrayMessage creates an array message
func NewArrayMessage(data []interface{}) Message {
	return Message{Type: "Array", Data: data}
}

// NewStreamMessage creates a stream message with byte data
func NewStreamMessage(data []byte) Message {
	// Convert bytes to array of numbers for JSON compatibility
	numbers := make([]int, len(data))
	for i, b := range data {
		numbers[i] = int(b)
	}
	return Message{Type: "Stream", Data: numbers}
}

// NewOptionalMessage creates an optional message
func NewOptionalMessage(data interface{}) Message {
	return Message{Type: "Optional", Data: data}
}

// NewAnyMessage creates a message with arbitrary data
func NewAnyMessage(data interface{}) Message {
	return Message{Type: "Any", Data: data}
}

// NewErrorMessage creates an error message
func NewErrorMessage(error string) Message {
	return Message{Type: "Error", Data: error}
}

// ToSerializable converts a Message to a format compatible with JSON serialization
func (m Message) ToSerializable() interface{} {
	switch m.Type {
	case "Flow":
		// Flow messages don't have data in the JSON format expected by Rust
		return map[string]interface{}{
			"type": "Flow",
			"data": nil,
		}
	case "Stream":
		// Convert bytes to array of integers for JSON compatibility
		if bytes, ok := m.Data.([]byte); ok {
			nums := make([]int, len(bytes))
			for i, b := range bytes {
				nums[i] = int(b)
			}
			return map[string]interface{}{
				"type": "Stream",
				"data": nums,
			}
		}
		return map[string]interface{}{
			"type": m.Type,
			"data": m.Data,
		}
	default:
		// For all other types, return as-is with the tagged structure
		return map[string]interface{}{
			"type": m.Type,
			"data": m.Data,
		}
	}
}

// ActorConfig contains configuration for the actor
type ActorConfig struct {
	NodeID      string                 `json:"node_id"`
	Component   string                 `json:"component"`
	ResolvedEnv map[string]string      `json:"resolved_env"`
	Config      map[string]interface{} `json:"config"`
	Namespace   *string                `json:"namespace"`
}

// GetString gets a string value from config
func (c *ActorConfig) GetString(key string) (string, bool) {
	if val, ok := c.Config[key]; ok {
		if str, ok := val.(string); ok {
			return str, true
		}
	}
	return "", false
}

// GetFloat gets a float value from config
func (c *ActorConfig) GetFloat(key string) (float64, bool) {
	if val, ok := c.Config[key]; ok {
		if num, ok := val.(float64); ok {
			return num, true
		}
	}
	return 0, false
}

// GetInt gets an integer value from config
func (c *ActorConfig) GetInt(key string) (int64, bool) {
	if val, ok := c.Config[key]; ok {
		if num, ok := val.(float64); ok {
			return int64(num), true
		}
	}
	return 0, false
}

// GetBool gets a boolean value from config
func (c *ActorConfig) GetBool(key string) (bool, bool) {
	if val, ok := c.Config[key]; ok {
		if b, ok := val.(bool); ok {
			return b, true
		}
	}
	return false, false
}

// GetEnv gets an environment variable value
func (c *ActorConfig) GetEnv(key string) (string, bool) {
	val, ok := c.ResolvedEnv[key]
	return val, ok
}

// ActorContext contains the context passed to actor processing functions
type ActorContext struct {
	Payload map[string]Message `json:"payload"`
	Config  ActorConfig        `json:"config"`
	State   interface{}        `json:"state"`
}

// ActorResult contains the result of actor processing
type ActorResult struct {
	Outputs map[string]interface{} `json:"outputs"`
	State   interface{}            `json:"state,omitempty"`
}

// PortDefinition defines an input or output port
type PortDefinition struct {
	Name        string `json:"name"`
	Description string `json:"description"`
	PortType    string `json:"port_type"`
	Required    bool   `json:"required"`
}

// PluginMetadata contains metadata about the plugin
type PluginMetadata struct {
	Component    string           `json:"component"`
	Description  string           `json:"description"`
	Inports      []PortDefinition `json:"inports"`
	Outports     []PortDefinition `json:"outports"`
	ConfigSchema interface{}      `json:"config_schema,omitempty"`
}

// ProcessFunc is the signature for actor processing functions
type ProcessFunc func(context ActorContext) (ActorResult, error)

// Global variables to store plugin configuration
var (
	pluginMetadata PluginMetadata
	processFunc    ProcessFunc
)

// RegisterPlugin registers a plugin with its metadata and processing function
func RegisterPlugin(metadata PluginMetadata, process ProcessFunc) {
	pluginMetadata = metadata
	processFunc = process
}

// GetMetadata is called by the exported get_metadata function
func GetMetadata() int32 {
	data, err := json.Marshal(pluginMetadata)
	if err != nil {
		pdk.SetError(fmt.Errorf("Failed to marshal plugin metadata: %v", err))
		return 1
	}

	// Output the raw bytes
	pdk.Output(data)
	return 0
}

// Process is called by the exported process function
func Process() int32 {
	input := pdk.Input()

	var context ActorContext
	if err := json.Unmarshal(input, &context); err != nil {
		pdk.SetError(fmt.Errorf("Failed to unmarshal actor context: %v", err))
		return 1
	}

	result, err := processFunc(context)
	if err != nil {
		pdk.SetError(fmt.Errorf("Actor processing error: %v", err))
		return 1
	}

	data, err := json.Marshal(result)
	if err != nil {
		pdk.SetError(fmt.Errorf("Failed to marshal actor result: %v", err))
		return 1
	}

	// Output the raw bytes
	pdk.Output(data)
	return 0
}

// Helper function to create port definitions
func NewPort(name, description, portType string, required bool) PortDefinition {
	return PortDefinition{
		Name:        name,
		Description: description,
		PortType:    portType,
		Required:    required,
	}
}

// Helper function to create required port definitions
func NewRequiredPort(name, description, portType string) PortDefinition {
	return NewPort(name, description, portType, true)
}

// Helper function to create optional port definitions
func NewOptionalPort(name, description, portType string) PortDefinition {
	return NewPort(name, description, portType, false)
}
