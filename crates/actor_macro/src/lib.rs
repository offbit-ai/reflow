use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{ItemFn, LitInt, Token, parse::Parse, parse::ParseStream, parse_macro_input};

/// Delivery semantics for a port connection.
#[derive(Debug, Clone, PartialEq)]
enum PortDelivery {
    /// Block if channel full. Messages never dropped. (default)
    Reliable,
    /// try_send — drop if channel full. For ticks, signals.
    Latest,
    /// Write to shared FramePool, send slot index. For large binary data.
    Pool(String),
}

impl Default for PortDelivery {
    fn default() -> Self {
        PortDelivery::Reliable
    }
}

#[derive(Debug, Clone)]
struct PortDef {
    name: String,
    delivery: PortDelivery,
}

#[derive(Debug, Default)]
struct PortsDefinition {
    capacity: Option<usize>,
    ports: Vec<String>,
    port_defs: Vec<PortDef>,
}

/// Parse a single port entry: `name` or `name: latest` or `name: pool("pool_name")`
fn parse_port_entry(input: ParseStream) -> syn::Result<PortDef> {
    let name = input.parse::<syn::Ident>()?.to_string();

    // Check for `: delivery` annotation
    let delivery = if input.peek(Token![:]) && !input.peek2(Token![:]) {
        input.parse::<Token![:]>()?;
        let kind = input.parse::<syn::Ident>()?;
        match kind.to_string().as_str() {
            "latest" => PortDelivery::Latest,
            "reliable" => PortDelivery::Reliable,
            "pool" => {
                // Parse pool("name")
                let content;
                syn::parenthesized!(content in input);
                let pool_name = content.parse::<syn::LitStr>()?;
                PortDelivery::Pool(pool_name.value())
            }
            other => {
                return Err(syn::Error::new(
                    kind.span(),
                    format!(
                        "Unknown port delivery kind '{}'. Expected 'latest', 'reliable', or 'pool(\"name\")'",
                        other
                    ),
                ));
            }
        }
    } else {
        PortDelivery::Reliable
    };

    Ok(PortDef { name, delivery })
}

impl Parse for PortsDefinition {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Parse the capacity in angle brackets, default to 0 if not provided
        let mut capacity = None;
        if input.peek(syn::token::Colon) {
            input.parse::<syn::token::Colon>()?;
            input.parse::<syn::token::Colon>()?;

            let _lt = input.parse::<Token![<]>()?;
            capacity = Some(input.parse::<LitInt>()?.base10_parse()?);
            let _gt = input.parse::<Token![>]>()?;
        }

        // Parse port entries in parentheses
        let content;
        syn::parenthesized!(content in input);

        let mut port_defs = Vec::new();
        while !content.is_empty() {
            port_defs.push(parse_port_entry(&content)?);
            if !content.is_empty() {
                content.parse::<Token![,]>()?;
            }
        }

        let ports = port_defs.iter().map(|p| p.name.clone()).collect();

        Ok(PortsDefinition {
            capacity,
            ports,
            port_defs,
        })
    }
}

struct ActorArgs {
    name: Option<syn::Ident>,
    _state: Option<syn::Ident>,
    inports: PortsDefinition,
    outports: PortsDefinition,
    await_all_inports: bool,
    await_inports: Vec<String>,
}

impl Parse for ActorArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut inports = PortsDefinition::default();
        let mut outports = PortsDefinition::default();
        let mut _state = None;
        let mut await_all_inports = false;
        let mut await_inports: Vec<String> = Vec::new();

        // Parse optional struct name
        if !input.peek(syn::token::Paren) {
            name = Some(input.parse::<syn::Ident>()?);
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        // Parse inports and outports
        while !input.is_empty() {
            let ident = input.parse::<syn::Ident>()?;

            match ident.to_string().as_str() {
                "state" => {
                    let content;
                    syn::parenthesized!(content in input);
                    let state_ident = content.parse::<syn::Ident>()?;
                    _state = Some(state_ident);
                }
                "inports" => {
                    let port_def = input.parse::<PortsDefinition>()?;
                    inports = port_def;
                }
                "outports" => {
                    let port_def = input.parse::<PortsDefinition>()?;
                    outports = port_def;
                }
                "await_all_inports" => {
                    await_all_inports = true;
                }
                "await_inports" => {
                    // Parse: await_inports(port1, port2, port3)
                    let content;
                    syn::parenthesized!(content in input);
                    let ports = Punctuated::<syn::Ident, Token![,]>::parse_terminated(&content)?;
                    await_inports = ports.into_iter().map(|i| i.to_string()).collect();
                }
                _ => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "Expected 'inports', 'outports', 'await_all_inports', or 'await_inports'",
                    ));
                }
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(ActorArgs {
            name,
            _state,
            inports,
            outports,
            await_all_inports,
            await_inports,
        })
    }
}

#[proc_macro_attribute]
pub fn actor(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as ActorArgs);
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let fn_vis = &input_fn.vis;

    // Create struct name from either provided name or function name
    let struct_name = match args.name {
        Some(name) => name,
        None => format_ident!(
            "{}Actor",
            fn_name
                .to_string()
                .chars()
                .next()
                .unwrap()
                .to_uppercase()
                .to_string()
                + &fn_name.to_string()[1..]
        ),
    };
    // Generate port initialization code
    let init_inports = args.inports.ports.iter().map(|port| {
        let name = port;
        quote! {
            String::from(#name)
        }
    });

    let init_outports = args.outports.ports.iter().map(|port| {
        let name = port;
        quote! {
            String::from(#name)
        }
    });

    let out_ports_cap = args.outports.capacity;
    let in_ports_cap = args.inports.capacity;
    let await_all_inports = args.await_all_inports;
    let await_inports_list = &args.await_inports;
    let _has_selective_await = !await_inports_list.is_empty();

    let out_ports_channel = if let Some(out_ports_cap) = out_ports_cap {
        if out_ports_cap < 1 {
            panic!("Outports capacity must be greater than 0");
        }
        quote! {flume::bounded(#out_ports_cap)}
    } else {
        quote! {flume::unbounded()}
    };
    // Actor inport channel is always unbounded — per-connector forwarder
    // channels handle backpressure via bounded(64) + delivery semantics.
    // The inport is just a merge point for all connectors, not a throttle.
    let in_ports_channel = quote! {flume::unbounded()};

    // Re-generate port name iterators for trait methods
    let inport_names_iter = args.inports.ports.iter().map(|port| {
        quote! { String::from(#port) }
    });
    let outport_names_iter = args.outports.ports.iter().map(|port| {
        quote! { String::from(#port) }
    });

    // Generate port delivery metadata entries
    let all_port_defs: Vec<&PortDef> = args
        .inports
        .port_defs
        .iter()
        .chain(args.outports.port_defs.iter())
        .collect();
    let port_delivery_entries = all_port_defs.iter().filter_map(|pd| {
        match &pd.delivery {
            PortDelivery::Reliable => None, // default, no entry needed
            PortDelivery::Latest => {
                let name = &pd.name;
                Some(quote! { m.insert(#name.to_string(), "latest".to_string()); })
            }
            PortDelivery::Pool(pool_name) => {
                let name = &pd.name;
                let pool = pool_name.as_str();
                Some(quote! { m.insert(#name.to_string(), format!("pool:{}", #pool)); })
            }
        }
    });

    let expanded = quote! {

        // Keep the original function
        #input_fn

        #fn_vis struct #struct_name {
            inports_channel: Port,
            outports_channel: Port,
        }

        impl #struct_name {
            pub fn new() -> Self {
                Self {
                    // NOTE: channels are intentionally cross-assigned —
                    // inports get the outport-declared capacity and vice
                    // versa.  Fixing this swap requires updating every
                    // actor's capacity declarations first (many use
                    // outports::<1> which would deadlock with bounded(1)).
                    inports_channel: #out_ports_channel,
                    outports_channel: #in_ports_channel,
                }
            }

            /// Get a list of available input ports
            pub fn input_ports(&self) -> Vec<String> {
                vec![#(#init_inports),*]
            }

            /// Get a list of available output ports
            pub fn output_ports(&self) -> Vec<String> {
                vec![#(#init_outports),*]
            }
        }

        impl Clone for #struct_name {
            fn clone(&self) -> Self {
                Self {
                    inports_channel: self.inports_channel.clone(),
                    outports_channel: self.outports_channel.clone(),
                }
            }
        }

        impl Actor for #struct_name {

            fn get_behavior(&self) -> ActorBehavior {
                Box::new(|context: ActorContext| {
                    Box::pin(async move {
                        #fn_name(context).await
                    })
                })
            }

            fn get_outports(&self) -> Port {
                self.outports_channel.clone()
            }

            fn get_inports(&self) -> Port {
                self.inports_channel.clone()
            }

            fn inport_names(&self) -> Vec<String> {
                vec![#(#inport_names_iter),*]
            }

            fn outport_names(&self) -> Vec<String> {
                vec![#(#outport_names_iter),*]
            }

            fn await_all_inports(&self) -> bool {
                #await_all_inports
            }

            fn required_inports(&self) -> Vec<String> {
                vec![#(String::from(#await_inports_list)),*]
            }

            fn port_delivery(&self) -> std::collections::HashMap<String, String> {
                let mut m = std::collections::HashMap::new();
                #(#port_delivery_entries)*
                m
            }

            fn create_instance(&self) -> std::sync::Arc<dyn Actor> {
                std::sync::Arc::new(Self::new())
            }

            // create_process() and create_state() use the trait defaults
            // via ActorProcess. Override only for non-MemoryState state types.
        }
    };

    TokenStream::from(expanded)
}
