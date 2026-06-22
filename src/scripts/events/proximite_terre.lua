-- Pause le jeu quand un vaisseau franchit le seuil de proximité de la Terre.
-- Le seuil est la portée du capteur le plus puissant du vaisseau (via declare_components).
-- Si le vaisseau n'a aucun capteur déclaré, il est ignoré.
-- La pause ne se déclenche qu'au premier passage sous le seuil.
-- Si le jeu démarre avec le vaisseau déjà proche, aucune pause.

-- Etat par vaisseau : nil = premier tick (pas encore de référence), true/false ensuite
local etait_proche = {}

on("ship_tick", function(data)
    local d = data.bodies["terre"]
    if d == nil then return end

    -- Seuil dynamique : portée du capteur le plus puissant du vaisseau
    local seuil = get_max_sensor_range(data.ship_id)
    if seuil == 0 then return end  -- pas de capteur déclaré pour ce vaisseau

    local actuel    = d < seuil
    local precedent = etait_proche[data.ship_id]

    -- Franchissement du seuil : était loin (false), maintenant proche (true)
    -- precedent == nil au démarrage → condition fausse → pas de pause si déjà proche
    if precedent == false and actuel then
        fire("pause_game", {})
    end

    etait_proche[data.ship_id] = actuel
end)
