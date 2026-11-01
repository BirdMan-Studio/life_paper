def main(game):
    me = game.me
    tile = game.world.get_tile(me.position.x, me.position.y)
    game.memory["calls"] = game.memory.get("calls", 0) + 1
    game.log(me.name, tile.terrain, game.memory["calls"])
