function init_game(canvas_id) {
    const canvas = document.getElementById(canvas_id);
    const ctx = canvas.getContext('2d');

    // const uri = 'wss://' + location.host + '/join/observer';
    const uri = 'wss://game-server-achtung.fly.dev/join/observer';
    const ws = new WebSocket(uri);
    var my_id = null;
    var game_state = null;

    function handleInitialStateEvent(event) {
        game_state = event.state;
        drawGameState();
    }

    function handleUpdateStateEvent(event) {
        for (const [player_id, player_diff] of Object.entries(event.diff.players)) {
            // Update body
            if (player_diff.body != null) {
                for (const blob of player_diff.body)
                    game_state.players[player_id].body.push(blob);
            }
            // Update other fields
            for (field of ['head', 'is_alive', 'size']) {
                if (player_diff[field] != null) {
                    game_state.players[player_id][field] = player_diff[field];
                }
            }
        }
        drawGameState();
    }

    function handleGameOverEvent(event) {
        ctx.font = "24px serif";
        ctx.fillStyle = "#ffffff";
        ctx.fillText("Winner: " + event.winner, 40, 40);
    }


    function drawGameState() {
        if (game_state == null) return;

        ctx.clearRect(0, 0, canvas.width, canvas.height);

        // dark blue background
        ctx.fillStyle = '#000033';
        ctx.fillRect(0, 0, canvas.width, canvas.height);

        for (const [player_id, player] of Object.entries(game_state.players)) {
            if (player_id == my_id) {
                ctx.fillStyle = '#ff0000';
            } else {
                ctx.fillStyle = '#00ccff';
            }
            for (const blob of player.body) {
                ctx.beginPath();
                ctx.arc(blob.position.x, blob.position.y, blob.size, 0, 2 * Math.PI);
                ctx.fill();
            }

            // Draw heads of alive players
            if (player.is_alive) {
                head = player.head;
                ctx.fillStyle = '#ffcc00';
                ctx.beginPath();
                ctx.arc(head.position.x, head.position.y, head.size, 0, 2 * Math.PI);
                ctx.fill();
            }
        }

    }

    ws.onmessage = async function (msg) {
        const data = await msg.data.text();
        const event = JSON.parse(data).event;
        switch (event.e) {
            case 'InitialState':
                handleInitialStateEvent(event);
                break;
            case 'AssignPlayerId':
                my_id = event.player_id;
                break;
            case 'UpdateState':
                handleUpdateStateEvent(event);
                break;
            case 'GameOver':
                handleGameOverEvent(event);
                break;
        }
    };

    ws.binaryType = "blob";
    ws.onopen = function () { console.log('Connected') };
    ws.onclose = function () { console.log('Disconnected'); };
}
