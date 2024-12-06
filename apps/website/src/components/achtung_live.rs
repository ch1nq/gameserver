use leptos::*;

/// Canvas that displays the live game
#[component]
pub fn AchtungLive() -> impl IntoView {
    view! {
        <div class="border border-gray-400 rounded-md w-full max-w-2xl aspect-square">
            <canvas
                id="game"
                width="1000"
                height="1000"
                class="w-full h-full max-w-full max-h-full"
            ></canvas>
        </div>
        <script src="achtung-observer.js"></script>
    }
}
