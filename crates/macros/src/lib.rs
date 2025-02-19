//! The macros used in Solipr.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Ident, Pat, Token, Type, Visibility};

/// Export a plugin function.
#[proc_macro_attribute]
pub fn export_fn(_: TokenStream, item: TokenStream) -> TokenStream {
    let data = syn::parse_macro_input!(item as syn::ItemFn);
    let intern_name = &data.sig.ident;
    let extern_name = quote::format_ident!("_wasm_guest_{intern_name}");

    let extern_fn = {
        let mut call = quote!();
        let mut args_type = quote!();
        for (i, arg) in data.sig.inputs.iter().enumerate() {
            let i = syn::Index::from(i);
            call.extend(quote!(args.#i,));
            if let syn::FnArg::Typed(arg_type) = arg {
                let ty = &arg_type.ty;
                args_type.extend(quote!(#ty,));
            }
        }
        quote!(
            #[no_mangle]
            extern "C" fn #extern_name(ptr: *mut u8, len: usize) -> u64 {
                let slice = unsafe { ::std::slice::from_raw_parts_mut(ptr, len) };
                let args: (#args_type) =
                    ::solipr_plugin::guest::__private::bincode::deserialize(slice).unwrap();
                if len != 0 {
                    unsafe {
                        ::std::alloc::dealloc(ptr, ::std::alloc::Layout::array::<u8>(len).unwrap())
                    };
                }

                let value = &#intern_name(#call);
                let len: usize = ::solipr_plugin::guest::__private::bincode::serialized_size(value)
                    .unwrap()
                    .try_into()
                    .unwrap();
                let ptr = unsafe {
                    if len == 0 {
                        ::std::ptr::null_mut()
                    } else {
                        ::std::alloc::alloc(::std::alloc::Layout::array::<u8>(len).unwrap())
                    }
                };
                let slice = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
                ::solipr_plugin::guest::__private::bincode::serialize_into(slice, value).unwrap();
                ((ptr as u64) << 32) | (len as u64)
            }
        )
    };

    quote!(#data #extern_fn).into()
}

/// Import a host function.
#[proc_macro_attribute]
pub fn import_fn(_: TokenStream, item: TokenStream) -> TokenStream {
    let data = syn::parse_macro_input!(item as syn::ForeignItemFn);
    let intern_name = &data.sig.ident;
    let intern_inputs = &data.sig.inputs;
    let intern_output = &data.sig.output;
    let intern_vis = &data.vis;
    let extern_name = quote::format_ident!("_wasm_host_{intern_name}");

    let mut intern_attrs = quote!();
    for attr in &data.attrs {
        intern_attrs.extend(quote!(#attr));
    }

    let mut inputs_tuple = quote!();
    for arg in intern_inputs {
        if let syn::FnArg::Typed(arg_type) = arg {
            if let Pat::Ident(ident) = &&*arg_type.pat {
                let ident = &ident.ident;
                inputs_tuple.extend(quote!(#ident,));
            }
        }
    }

    quote!(
        extern "C" {
            fn #extern_name(ptr: *mut u8, len: usize) -> u64;
        }
        #intern_attrs
        #intern_vis fn #intern_name(#intern_inputs) #intern_output {
            let value = &(#inputs_tuple);
            let len: usize = ::solipr_plugin::guest::__private::bincode::serialized_size(value)
                .unwrap()
                .try_into()
                .unwrap();
            let ptr = unsafe {
                if len == 0 {
                    ::std::ptr::null_mut()
                } else {
                    ::std::alloc::alloc(::std::alloc::Layout::array::<u8>(len).unwrap())
                }
            };
            let slice = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
            ::solipr_plugin::guest::__private::bincode::serialize_into(slice, value).unwrap();
            let value = unsafe { #extern_name(ptr, len) };
            if len != 0 {
                unsafe {
                    ::std::alloc::dealloc(ptr, ::std::alloc::Layout::array::<u8>(len).unwrap())
                };
            }

            let (ptr, len) = ((value >> 32_i64) as *mut u8, (value & 0xffff_ffff) as usize);
            let slice = unsafe { ::std::slice::from_raw_parts_mut(ptr, len) };
            let value = ::solipr_plugin::bincode::deserialize(slice).unwrap();
            if len != 0 {
                unsafe {
                    ::std::alloc::dealloc(ptr, ::std::alloc::Layout::array::<u8>(len).unwrap())
                };
            }
            value
        }
    )
    .into()
}

/// A declaration of a static variable without a value.
struct Declaration {
    /// The attributes of the declaration.
    attrs: Vec<Attribute>,

    /// The visibility of the declaration.
    vis: Visibility,

    /// The name of the declaration.
    ident: Ident,

    /// The type of the declaration.
    ty: Type,
}

impl Parse for Declaration {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis: Visibility = input.parse()?;
        input.parse::<Token![static]>()?;
        let ident: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        input.parse::<Token![;]>()?;

        Ok(Self {
            attrs,
            vis,
            ident,
            ty,
        })
    }
}

/// Define a data type that can be used by host functions.
#[proc_macro_attribute]
pub fn host_fn_registry(_: TokenStream, item: TokenStream) -> TokenStream {
    let Declaration {
        attrs,
        vis,
        ident,
        ty,
    } = syn::parse_macro_input!(item as Declaration);
    let mut intern_attrs = quote!();
    for attr in &attrs {
        intern_attrs.extend(quote!(#attr));
    }
    quote! {
        #intern_attrs
        #[::linkme::distributed_slice]
        #vis static #ident: [(
            &'static str,
            for<'store> fn(
                ::solipr_plugin::host::__private::PluginCtx<'store, #ty>,
                u32,
                u32,
            ) -> Box<dyn ::std::future::Future<Output = u64> + Send + 'store>,
        )];
    }
    .into()
}

/// Create a host function that can be called from plugin code.
#[proc_macro_attribute]
pub fn host_fn(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr: syn::Ident = syn::parse_macro_input!(attr as syn::Ident);
    let data: syn::ItemFn = syn::parse_macro_input!(item as syn::ItemFn);
    let intern_name = &data.sig.ident;
    let extern_name = quote::format_ident!("_wasm_host_{intern_name}");
    let extern_name_lit = syn::LitStr::new(&extern_name.to_string(), intern_name.span());
    let extern_static_name =
        quote::format_ident!("_WASM_HOST_{}", intern_name.to_string().to_uppercase());

    let extern_fn = {
        let mut data_type = quote!();
        let mut call = quote!();
        let mut args_type = quote!();
        for (i, arg) in data.sig.inputs.iter().enumerate() {
            if i == 0 {
                if let syn::FnArg::Typed(arg_type) = arg {
                    let ty = &arg_type.ty;
                    if let Type::Reference(ref_ty) = &**ty {
                        let ty = &ref_ty.elem;
                        data_type = quote!(#ty);
                    }
                }
                continue;
            }
            let i = syn::Index::from(i.saturating_sub(1));
            call.extend(quote!(args.#i,));
            if let syn::FnArg::Typed(arg_type) = arg {
                let ty = &arg_type.ty;
                args_type.extend(quote!(#ty,));
            }
        }
        quote!(
            fn #extern_name<'store>(
                mut ctx: ::solipr_plugin::host::__private::PluginCtx<'store, #data_type>,
                ptr: u32,
                len: u32,
            ) -> Box<dyn ::std::future::Future<Output = u64> + Send + 'store> {
                // Get the memory of the plugin.
                let Some(memory) = ctx.memory() else {
                    return Box::new(async { 0_u64 });
                };

                // Get the memory slice for the args.
                let Some(args_slice) = memory
                    .data(&*ctx)
                    .get(ptr as usize..(ptr.saturating_add(len) as usize))
                else {
                    return Box::new(async { 0_u64 });
                };

                // Deserialize the args from the memory slice.
                let Ok(args) =
                    ::solipr_plugin::host::__private::bincode::deserialize::<(#args_type)>
                    (args_slice) else {
                    return Box::new(async { 0_u64 });
                    // TEST
                };

                Box::new(async move {
                    // Execute the function.
                    let Ok(result) = #intern_name(ctx.data_mut(), #call).await else {
                        return 0_u64;
                    };

                    // Calculate the length of the result.
                    let Ok(len) =
                        ::solipr_plugin::host::__private::bincode::serialized_size(&result) else {
                        return 0_u64;
                    };
                    let Ok(len): Result<u32, _> = len.try_into() else {
                        return 0_u64;
                    };

                    // Allocate a new memory region for the result.
                    let Some(alloc) = ctx.alloc() else {
                        return 0_u64;
                    };
                    let Ok(ptr) = alloc.call_async(&mut *ctx, len).await else {
                        return 0_u64;
                    };

                    // Get the memory slice to write the result to.
                    let Some(result_slice) = memory
                        .data_mut(&mut *ctx)
                        .get_mut(ptr as usize..(ptr.saturating_add(len) as usize))
                    else {
                        return 0_u64;
                    };

                    // Serialize the result into the memory slice.
                    if ::solipr_plugin::host::__private::bincode::serialize_into
                        (result_slice, &result).is_err() {
                        return 0_u64;
                    }

                    // Return the result.
                    (u64::from(ptr) << 32_i64) | u64::from(len)
                })
            }

            #[::linkme::distributed_slice(#attr)]
            static #extern_static_name: (
                &'static str,
                for<'store> fn(
                    ::solipr_plugin::host::__private::PluginCtx<'store, #data_type>,
                    u32,
                    u32,
                ) -> Box<dyn ::std::future::Future<Output = u64> + Send + 'store>,
            ) = (#extern_name_lit, #extern_name);
        )
    };

    quote!(#data #extern_fn).into()
}
