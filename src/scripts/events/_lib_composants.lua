-- Bibliothèque de composants pour les vaisseaux.
-- Chargée en premier (préfixe _) : ses fonctions sont disponibles dans tous les scripts events/.
--
-- Un vaisseau déclare ses composants en appelant declare_components() au chargement du script.
-- Les fonctions use_thruster / empty_tank / detect_obstacle opèrent sur cet état persistant.
--
-- Unités :
--   distances : km
--   thrust    : km/jour (delta-v direct, même unité que apply_global_thrust)
--   carburant : litres (unité libre, définie par le script)

local _composants = {}  -- état indexé par ship_id

-- Déclare les composants d'un vaisseau.
-- config = {
--   tanks     = { id = { capacite=f, carburant=f } }
--   thrusters = { id = { force_max=f, consommation=f, reservoir=string } }
--   sensors   = { id = { portee=f } }
-- }
-- `consommation` : litres consommés par unité de force appliquée (force_max * power * consommation)
-- `reservoir`    : id du tank à débiter (défaut : "principal")
function declare_components(ship_id, config)
    _composants[ship_id] = {
        tanks     = config.tanks     or {},
        thrusters = config.thrusters or {},
        sensors   = config.sensors   or {},
    }
end

-- Renvoie la portée maximale parmi tous les capteurs du vaisseau (0 si aucun).
function get_max_sensor_range(ship_id)
    local c = _composants[ship_id]
    if c == nil then return 0 end
    local max_range = 0
    for _, sensor in pairs(c.sensors) do
        if sensor.portee > max_range then
            max_range = sensor.portee
        end
    end
    return max_range
end

-- Renvoie le carburant restant dans un tank (0 si inconnu).
function get_fuel(ship_id, tank_id)
    local c = _composants[ship_id]
    if c == nil then return 0 end
    local t = c.tanks[tank_id]
    if t == nil then return 0 end
    return t.carburant
end

-- Retire `volume` litres d'un tank.
-- Retourne true si le retrait a eu lieu, false si insuffisant ou inexistant.
function empty_tank(ship_id, tank_id, volume)
    local c = _composants[ship_id]
    if c == nil then return false end
    local t = c.tanks[tank_id]
    if t == nil or t.carburant < volume then return false end
    t.carburant = t.carburant - volume
    return true
end

-- Applique une poussée avec le moteur `thruster_id` à la puissance `power` (0..1)
-- dans la direction `dir` = {x, y, z} (sera normalisée si non nulle).
-- Consomme du carburant proportionnellement à la force réelle appliquée.
-- Retourne true si la poussée a été appliquée, false si pas de carburant ou moteur inconnu.
function use_thruster(ship_id, thruster_id, power, dir)
    local c = _composants[ship_id]
    if c == nil then return false end
    local thruster = c.thrusters[thruster_id]
    if thruster == nil then return false end

    local force   = thruster.force_max * power
    local fuel_id = thruster.reservoir or "principal"
    local fuel    = force * (thruster.consommation or 0)

    if fuel > 0 and not empty_tank(ship_id, fuel_id, fuel) then
        return false
    end

    -- Normalisation de la direction
    local len = math.sqrt(dir.x * dir.x + dir.y * dir.y + dir.z * dir.z)
    if len == 0 then return false end

    fire("apply_thrust", {
        ship_id = ship_id,
        dx = dir.x / len * force,
        dy = dir.y / len * force,
        dz = dir.z / len * force,
    })
    return true
end

-- Détecte les objets (corps célestes et vaisseaux) à portée du capteur `sensor_id`.
-- Retourne une liste de { id, type ("corps"|"vaisseau"), distance } triée par distance.
-- Retourne {} si le capteur est inconnu ou le vaisseau non déclaré.
function detect_obstacle(data, sensor_id)
    local c = _composants[data.ship_id]
    if c == nil then return {} end
    local sensor = c.sensors[sensor_id]
    if sensor == nil then return {} end

    local resultats = {}

    for id, dist in pairs(data.bodies) do
        if dist < sensor.portee then
            resultats[#resultats + 1] = { id = id, type = "corps", distance = dist }
        end
    end

    for id, dist in pairs(data.ships) do
        if dist < sensor.portee then
            resultats[#resultats + 1] = { id = id, type = "vaisseau", distance = dist }
        end
    end

    table.sort(resultats, function(a, b) return a.distance < b.distance end)
    return resultats
end
