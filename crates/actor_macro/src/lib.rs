#![feature(extend_one)]

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{parse::Parse, parse::ParseStream, parse_macro_input, ItemFn, LitInt, Token};

#[derive(Debug, Default)]
struct PortsDefinition {
    capacity: usize,
    ports: Vec<String>,
}

impl Parse for PortsDefinition {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Parse the capacity in angle brackets, default to <50> if not provided
        let mut capacity = 50;
        if input.peek(syn::token::Colon) {
            input.parse::<syn::token::Colon>()?;
            input.parse::<syn::token::Colon>()?;

            let _lt = input.parse::<Token![<]>()?;
            capacity = input.parse::<LitInt>()?.base10_parse()?;
            let _gt = input.parse::<Token![>]>()?;
        }

        // Parse the port names in parentheses (A, B)
        let content;
        syn::parenthesized!(content in input);
        let ports = Punctuated::<syn::Ident, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .map(|ident| ident.to_string())
            .collect();

        Ok(PortsDefinition { capacity, ports })
    }
}

struct ActorArgs {
    name: Option<syn::Ident>,
    state: Option<syn::Ident>,
    inports: PortsDefinition,
    outports: PortsDefinition,
    await_all_inports: bool,
}

impl Parse for ActorArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut inports = PortsDefinition::default();
        let mut outports = PortsDefinition::default();
        let mut state = None;
        let mut await_all_inports = false;

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
                    state = Some(state_ident);
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
                _ => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "Expected 'inports' or 'outports'",
                    ))
                }
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(ActorArgs {
            name,
            state,
            inports,
            outports,
            await_all_inports,
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
    let state_name = if args.state.is_none() {
        format_ident!("MemoryState")
    } else {
        args.state.clone().unwrap()
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

    let expanded = quote! {

            // Keep the original function
            #input_fn

            #fn_vis struct #struct_name {
                inports: Vec<String>,
                outports: Vec<String>,
                inports_channel: Port,
                outports_channel: Port,
                await_all_inports: bool,
            }

            impl #struct_name {
                pub fn new() -> Self {
                    Self {
                        inports: vec![#(#init_inports),*],
                        outports: vec![#(#init_outports),*],
                        inports_channel: flume::bounded(#in_ports_cap),
                        outports_channel: flume::bounded(#out_ports_cap),
                        await_all_inports: #await_all_inports
                    }
                }

                /// Get a list of available input ports
                pub fn input_ports(&self) -> Vec<String> {
                    self.inports.clone()
                }

                /// Get a list of available output ports
                pub fn output_ports(&self) -> Vec<String> {
                    self.outports.clone()
                }
            }

            impl Clone for #struct_name {
                fn clone(&self) -> Self {
                    Self {
                        inports: self.inports.clone(),
                        outports: self.outports.clone(),
                        inports_channel: self.inports_channel.clone(),
                        outports_channel: self.outports_channel.clone(),
                        await_all_inports: self.await_all_inports
                    }
                }
            }

            impl Actor for #struct_name {

                fn get_behavior(&self) -> ActorBehavior {
                    Box::new(#fn_name)
                }

                fn get_outports(&self) -> Port {
                    self.outports_channel.clone()
                }

                fn get_inports(&self) -> Port {
                    self.inports_channel.clone()
                }

                fn create_process(&self) ->  std::pin::Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>> {

                    let await_all_inports = self.await_all_inports;
                    let outports = self.get_outports();
                    let behavior = self.get_behavior();
                    let actor_state = std::sync::Arc::new(parking_lot::Mutex::new(#state_name::default()));

                    // let mut all_inports:std::rc::Rc<HashMap<String, Message>> =std::rc::Rc::new(HashMap::new());
                    let inports_size = self.input_ports().len();

                    let (_, receiver) = self.get_inports();

                    Box::pin(async move {
                        use futures::Stream;
                        use futures::StreamExt;
                        use serde_json::json;
                        use std::borrow::BorrowMut;

                        let behavior_func = behavior;
                        let mut all_inports = std::collections::HashMap::new();

                        loop {
                     
                            if let Some(packet) = receiver.clone().stream().next().await {
                                
                                if await_all_inports {
                                    if all_inports.keys().len() < inports_size  {
                                        all_inports.extend(packet.iter().map(|(k, v)| {(k.clone(), v.clone())}));
                                        if all_inports.keys().len() == inports_size  {
                                            // Run the behavior function
                                            if let Ok(result) = (behavior_func)(all_inports.clone(), actor_state.clone(), outports.clone())
                                            {
                                                if !result.is_empty() {
                                                    let _ = outports.0.send(result)
                                                        .expect("Expected to send message via outport");
                                                }
                                            }
                                        }
                                        continue;
                                    }
                                }

                                
                                if(!await_all_inports) {
                                    // Run the behavior function
                                    if let Ok(result) = (behavior_func)(packet, actor_state.clone(), outports.clone())
                                    {
                                        if !result.is_empty() {
                                            let _ = outports.0.send(result)
                                                .expect("Expected to send message via outport");
                                        }
                                    }
                                }
                                
                            }
                        }
                    })
                }

            }
        };

    TokenStream::from(expanded)
}
