  1. subsecond — adopt directly (highest value)                                           
  
  The biggest surprise from the research. subsecond is a standalone hot-patching crate    
  that reloads Rust code in-process using stack rewinding — no process kill, no full
  recompile cycle. It's explicitly designed to be used outside Dioxus (their docs call out
   webservers, game loops, request handlers).                                             
  
  Prism's current dev loop is WatchLoop → kill child → cargo run again. With subsecond,   
  the shell process stays alive and only the changed function pointers are swapped. For a
  GPU-rendered retained-mode UI this matters a lot: the winit event loop, the femtovg
  context, and the Surface tree all survive the reload.                                   
  
  How to integrate: Wire subsecond into prism-cli's DevLoop as the primary hot-patch path 
  for .rs changes. Keep the full respawn fallback for struct layout changes that subsecond
   can't handle. Tag reload anchor points in prism-shell with subsecond::call() at the
  component render boundary.                                                              
  
  ---                                                                                     
  2. generational-box — adopt as the memory primitive for Prism signals
                                                                       
  generational-box is published independently and is what Dioxus signals are built on.
  It's an arena allocator with generational indices that lets you hold stable typed
  references to values across the reactive graph without lifetime gymnastics. The
  Owner/Storage/SyncStorage/UnsyncStorage type split gives you a clean sync vs. send
  boundary.                                                                               
  
  Prism needs a reactivity layer that bridges CRDT changes into UI re-renders.            
  dioxus-signals itself is too coupled to Dioxus's virtual DOM scheduler to adopt
  wholesale, but generational-box is the right foundation to build
  prism-core::signal::{Signal, Memo, Effect} on top of — with Loro CRDT as the            
  authoritative source and signals as the derived read layer.
                                                                                          
  How to integrate: Add generational-box to prism-core. Build a Signal<T> that wraps a
  Loro container value, a Memo<T> for derived state, and an Effect for side effects
  (re-renders). CRDT transactions commit to Loro first; signals invalidate subscribers
  second. This gives you fine-grained reactivity without a full virtual DOM.              
  
  ---                                                                                     
  3. Server functions pattern — implement in prism-daemon IPC   
                                                             
  Dioxus server functions: annotate a function with #[server], a proc-macro generates a
  type-safe client stub that serializes args over the wire and a server impl that handles
  the request. The client never writes serialization code by hand.                        
  
  Prism has #[daemon_command] for registering server-side handlers but the client side    
  still speaks raw JSON. The server function pattern closes this gap: extend
  prism-luau-derive with a #[daemon_fn] macro that generates both the IPC client stub     
  (serializes via postcard, sends over the transport-ipc socket, deserializes the         
  response) and the server registration — so calling a daemon command from the shell side
  looks like a normal async Rust function call.                                           
                                                                
  How to integrate: New derive path in prism-luau-derive. The #[daemon_command] macro
  already captures the function signature; add a client output that generates the matching
   stub. Studio and shell get typed calls; the JSON/postcard boundary stays invisible.
                                                                                          
  ---
  4. Typesafe routing — implement for app page navigation                                 
                                                                
  Dioxus router encodes routes as an enum where each variant is a URL segment, derives
  Routable, and gets typed Link<Route::Page> navigation. No stringly-typed route strings.
                                          
  Prism's apps have per-app page navigation (per the memory note: tabs are per-app page
  navigation, not Studio-wide). Right now that's presumably an untyped string or enum.    
  Implementing the same derive pattern — #[derive(Routable)] on an app's page enum — gives
   compile-time route exhaustiveness, typed nav calls, and the same enum works for web URL
   routing and in-app state routing without duplication.        
                                                                                          
  How to integrate: New prism-core::routing module. One derive macro, one Navigator<R: 
  Routable> type. Web target maps routes to URL hash/path; desktop/studio target manages
  them as app state. No Dioxus dependency needed — the design is the borrowing.
                                                                                          
  ---
  5. RSX hot-reload template hashing — for .prism-ui DSL                                  
                                                                
  Dioxus's rsx-hotreload works by hashing each RSX template at build time, then at runtime
   diffing the new hash against the cached one and only re-evaluating templates that
  changed. The key insight: you don't need to recompile a changed template if the         
  structure is the same and only literal values (strings, colors, sizes) changed — those
  can be patched in place.                                                                
                                                                
  The .prism-ui DSL has the same opportunity. Most hot-reload changes in a UI file are    
  literal value tweaks — a color, a padding value, a font size — not structural rewrites.
  Hashing templates at parse time and doing patch-in-place for literal-only changes means
  the Surface tree doesn't need a full re-walk.                                           
  
  How to integrate: Add a template hash to prism-ui-build's codegen output. In the        
  runtime's lower_ui pipeline, check the hash before full re-evaluation and fast-path
  literal-value patches. Pairs naturally with subsecond for .rs changes.
                                                                                          
  ---
  Watch list (not ready, but worth tracking)                                              
                                                                
  - Blitz / dioxus-native: Pre-alpha native GPU renderer using Vello + WGPU. If it matures
   it could be relevant for the SSR semantic HTML path, but Prism's femtovg renderer is
  further along for native. Revisit at stable.                                            
  - manganis: Asset pipeline with build-time hashing and CDN optimization. Worth adopting
  when Prism's web target has non-trivial asset management needs.                         
  - wasm-split: Lazy WASM module loading. Matters when prism_shell.wasm gets large enough
  that initial load time is a problem.                                                    
                                                                                          
  ---
  Priority order                                                                          
                                                                
  ┌─────┬────────────────────────────────────────┬────────────────────────────────────────────┬──────────────────────────┐
  │  #  │                  What                  │                    How                     │          Effort          │
  ├─────┼────────────────────────────────────────┼────────────────────────────────────────────┼──────────────────────────┤
  │ 1   │ subsecond in dev loop                  │ Add dep, tag anchor points in shell        │ Low — it's a dep drop-in │
  ├─────┼────────────────────────────────────────┼────────────────────────────────────────────┼──────────────────────────┤
  │ 2   │ generational-box + Prism signals       │ New prism-core::signal module              │ Medium                   │
  ├─────┼────────────────────────────────────────┼────────────────────────────────────────────┼──────────────────────────┤
  │ 3   │ Server function pattern for daemon IPC │ Extend prism-luau-derive                   │ Medium                   │
  ├─────┼────────────────────────────────────────┼────────────────────────────────────────────┼──────────────────────────┤
  │ 4   │ .prism-ui template hashing             │ prism-ui-build codegen + runtime fast-path │ Medium                   │
  ├─────┼────────────────────────────────────────┼────────────────────────────────────────────┼──────────────────────────┤
  │ 5   │ Typesafe routing                       │ New prism-core::routing                    │ Low                      │
  └─────┴────────────────────────────────────────┴────────────────────────────────────────────┴──────────────────────────┘