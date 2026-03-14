import { PortViewTrait } from "./port.js"

export const NodeRepository = [
    {
        name: "Interaction",
        process: "interaction",
        inports: [
            {
                name: "Layer", trait: PortViewTrait.OPTION,
                viewModel: [
                    {
                        id: "my_button",
                        displayText: "Like Button",
                        value: "my_button_data",
                        selected: true
                    },
                    {
                        id: "my_slider",
                        displayText: "Slider",
                        value: 0,
                        selected: false
                    }
                ],
                hideLabel: true
            },
            { name: "Enable", trait: PortViewTrait.BOOLEAN, viewModel: true }
        ],
        outports: [
            { name: "Down", trait: PortViewTrait.BOOLEAN, viewModel: false },
            { name: "Tap", trait: PortViewTrait.PULSE, viewModel: false },
            { name: "Position", trait: PortViewTrait.STANDARD },
            { name: "Force", trait: PortViewTrait.NUMBER_INPUT, viewModel: 0 }
        ]
    },
    {
        name: "HTTP POST Request",
        process: "http_post",
        inports: [
            {
                name: "Endpoint",
                trait: PortViewTrait.TEXT_INPUT,
                viewModel:"https://#"
            },
            {
                name: "Trigger",
            }
        ],
        outports: [{ name: "Done" }],
       
    }
]