-- Pause le jeu quand un vaisseau franchit le seuil de proximité de la Terre.
-- La pause ne se déclenche qu'au premier passage sous le seuil.
-- Si le jeu démarre avec le vaisseau déjà proche, aucune pause.

local SEUIL = 100000.0  -- distance critique en km (modifiable ici)

-- Etat par vaisseau : nil = premier tick (pas encore de référence), true/false ensuite
local etait_proche = {}

on("ship_tick", function(data)
    local d = data.bodies["terre"]
    if d == nil then return end

    local actuel = d < SEUIL
    local precedent = etait_proche[data.ship_id]  -- nil au premier tick

    -- Franchissement du seuil : était loin (false), maintenant proche (true)
    -- precedent == nil au démarrage → condition fausse → pas de pause si déjà proche
    if precedent == false and actuel then
        fire("pause_game", {})
    end

    etait_proche[data.ship_id] = actuel
end)
