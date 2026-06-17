-- Exemple de script de comportement pour le vaisseau "shp".
-- Illustre l'utilisation de _lib_composants.lua.
--
-- Pour adapter à un autre vaisseau : changer "shp" par l'ID du vaisseau concerné,
-- et ajuster les valeurs de configuration ci-dessous.

-- Déclaration des composants du vaisseau "shp"
declare_components("shp", {
    tanks = {
        principal = { capacite = 1000.0, carburant = 800.0 },
    },
    thrusters = {
        main = {
            force_max    = 50.0,   -- km/jour de delta-v max par tick
            consommation = 0.01,   -- litres par km/jour de force
            reservoir    = "principal",
        },
    },
    sensors = {
        radar = { portee = 50000.0 },  -- 50 000 km
    },
})

-- Comportement : chaque tick de simulation
on("ship_tick", function(data)
    if data.ship_id ~= "shp" then return end

    -- Détecter tous les objets à portée radar
    local proches = detect_obstacle(data, "radar")

    for _, obj in ipairs(proches) do
        -- Pause si un corps céleste est à moins de 10 000 km
        if obj.type == "corps" and obj.distance < 10000.0 then
            fire("pause_game", {})
            return
        end
    end

    -- Exemple d'utilisation de use_thruster (désactivé par défaut)
    -- use_thruster("shp", "main", 0.5, { x = 1.0, y = 0.0, z = 0.0 })
end)
