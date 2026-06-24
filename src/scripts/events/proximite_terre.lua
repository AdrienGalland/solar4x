-- Pause le jeu quand un vaisseau franchit le seuil de proximité de la Terre.
-- Le seuil est la portée du capteur le plus puissant du vaisseau (via declare_components).
-- Si le vaisseau n'a aucun capteur déclaré, il est ignoré.
-- La pause ne se déclenche qu'au premier passage sous le seuil.
-- Si le jeu démarre avec le vaisseau déjà proche, aucune pause.

-- Etat par vaisseau : nil = premier tick (pas encore de référence), true/false ensuite
local etait_proche = {}

local _log_tick = {}  -- DEBUG: limite les logs à 1 par seconde par vaisseau

on("ship_tick", function(data)
    local d = data.bodies["terre"]
    if d == nil then return end

    -- Seuil dynamique : portée du capteur le plus puissant du vaisseau
    local seuil = get_max_sensor_range(data.ship_id)

    -- DEBUG: affiche distance + seuil toutes les 60 ticks environ
    _log_tick[data.ship_id] = (_log_tick[data.ship_id] or 0) + 1
    if _log_tick[data.ship_id] % 60 == 1 then
        print("[proximite_terre] " .. data.ship_id
              .. " dist_terre=" .. string.format("%.0f", d) .. " km"
              .. "  seuil=" .. seuil .. " km")
    end

    if seuil == 0 then return end  -- pas de capteur déclaré pour ce vaisseau

    local actuel    = d < seuil
    local precedent = etait_proche[data.ship_id]

    if precedent == false and actuel then
        print("[proximite_terre] PAUSE déclenché pour " .. data.ship_id)
        fire("pause_game", {})
    end

    etait_proche[data.ship_id] = actuel
end)
