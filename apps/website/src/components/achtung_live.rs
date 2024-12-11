use leptos::prelude::*;

/// Canvas that displays the live game
#[component]
pub fn AchtungLive() -> impl IntoView {
    view! {
        <div class="flex flex-col lg:flex-row gap-4">
            <div class="border rounded-lg aspect-square overflow-hidden w-full">
                <canvas
                    id="game"
                    width="1000"
                    height="1000"
                    class="max-h-full h-full max-w-full w-full"
                ></canvas>
                <script src="achtung-observer.js"></script>
                <script>{r#"init_game('game')"#}</script>
            </div>
            <div class="border rounded-lg col-span-full border-gray-300 bg-gray-100 w-full flex-grow h-fit">
                <table class="w-full text-left">
                    <thead class="font-semibold pl-4 py-2 mb-3 border-b border-gray-300 text-gray-800">
                        <tr class="uppercase text-sm">
                            <th class="pl-4 py-2">Color</th>
                            <th>Name</th>
                            <th>Owner</th>
                            <th>Global Rank</th>
                            <th>Win-rate (Recent)</th>
                        </tr>
                    </thead>
                    <tbody>
                        {(0..8)
                            .map(|i| {
                                let color = "background: hsl(200, 70%, 50%)";
                                view! {
                                        <tr class="border-b">
                                            <th class="px-4 py-3">
                                                <span
                                                    class="w-10 h-2 rounded-full block"
                                                    style=color
                                                ></span>
                                            </th>
                                            <th class="whitespace-nowrap font-normal">agent-{i}</th>
                                            <th class="whitespace-nowrap font-normal">user-{i}</th>
                                            // <th class="whitespace-nowrap font-normal">#{rand::random::<u16>() % 50}</th>
                                            // <th class="whitespace-nowrap font-normal">{rand::random::<u16>() % 100}%</th>
                                        </tr>
                                }
                            })
                            .collect_view()}
                    </tbody>
                </table>
            </div>
        </div>
    }
}
