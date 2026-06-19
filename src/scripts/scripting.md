# Guide de scripting Lua — Solar4X

Solar4X expose deux systèmes de scripting complémentaires. Chacun sert un usage différent et dispose de ses propres fonctions.

---

## Vue d'ensemble

| Système | Dossier | Usage |
|---|---|---|
| **Scripts de comportement** | `src/scripts/events/` | Réagir à des événements, gérer les composants du vaisseau (moteurs, réservoirs, capteurs), logique autonome avec état persistant |
| **Scripts de propulsion** | `src/scripts/ships/` | Calculer et appliquer une poussée à chaque tick de simulation, logique sans état |

> **Règle simple :** si ton script doit se souvenir de quelque chose entre deux ticks (niveau de carburant, état d'une manœuvre en cours…), utilise un script de comportement. Si tu calcules juste une poussée à appliquer maintenant, utilise un script de propulsion.

---

## 1. Scripts de comportement (`src/scripts/events/`)

Ces scripts sont chargés une seule fois au démarrage de la partie et fonctionnent en continu. Ils s'exécutent dans un **bus d'événements** persistent : les variables locales déclarées en dehors des handlers sont conservées entre les frames.

Les fichiers sont chargés par ordre alphabétique. Les fichiers préfixés par `_` (ex. `_lib_composants.lua`) sont garantis de se charger en premier.

### 1.1 Fonctions du bus d'événements

#### `on(event, fn)` 

Enregistre un handler appelé à chaque fois que l'événement `event` est tiré.

```lua
on("ship_tick", function(data)
    -- appelé chaque tick de simulation pour chaque vaisseau
end)
```

Le handler s'exécute comme une **coroutine** : il peut se suspendre avec `wait_for`.

#### `fire(event, data)`

Tire un événement avec une table de données. Les autres scripts qui ont fait `on(event, …)` seront appelés au prochain frame. Certains noms d'événements ont un effet direct sur le jeu (voir section 1.3).

```lua
fire("mon_evenement", { valeur = 42 })
fire("pause_game", {})
```

#### `wait_for(event)`

Suspend le handler courant jusqu'à ce que l'événement `event` soit tiré. Le reste du code s'exécute quand l'événement arrive.

```lua
on("debut_manoeuvre", function(data)
    -- lancer la manœuvre...
    wait_for("manoeuvre_terminee")
    -- ce code s'exécute seulement après la fin de la manœuvre
    fire("pause_game", {})
end)
```

> `wait_for` ne peut être utilisé qu'à l'intérieur d'un handler `on(...)`.

---

### 1.2 Événement `ship_tick`

Tiré automatiquement à chaque tick de simulation pour chaque vaisseau, tant que le temps tourne.

**Structure des données reçues :**

```lua
on("ship_tick", function(data)
    data.ship_id          -- string : identifiant du vaisseau (ex. "shp")
    data.bodies           -- table  : { body_id → distance en km }
    data.ships            -- table  : { ship_id → distance en km }
end)
```

**Exemple — filtrer par vaisseau :**

```lua
on("ship_tick", function(data)
    if data.ship_id ~= "shp" then return end  -- n'agir que pour "shp"

    local d_terre = data.bodies["terre"]       -- distance à la Terre en km
    local d_autre = data.ships["autre_vaisseau"]
end)
```

---

### 1.3 Événements système (actions sur le jeu)

Ces noms d'événements sont interceptés par le moteur et déclenchent des actions directes :

| Événement | Effet |
|---|---|
| `fire("pause_game", {})` | Met le temps en pause |
| `fire("resume_game", {})` | Remet le temps en marche |
| `fire("apply_thrust", { ship_id, dx, dy, dz })` | Applique une impulsion au vaisseau `ship_id` (en km/jour) |

**Exemple — apply_thrust directement :**

```lua
fire("apply_thrust", {
    ship_id = "shp",
    dx = 10.0,   -- km/jour en x
    dy = 0.0,
    dz = 0.0,
})
```

> En pratique, préfère `use_thruster` (voir section 1.4) qui gère automatiquement la consommation de carburant.

---

### 1.4 Bibliothèque de composants (`_lib_composants.lua`)

Disponible dans tous les scripts de comportement. Permet de déclarer les composants d'un vaisseau (moteurs, réservoirs, capteurs) et de les utiliser via des fonctions utilitaires.

#### `declare_components(ship_id, config)`

Déclare les composants d'un vaisseau. À appeler **une seule fois**, en dehors de tout handler.

```lua
declare_components("shp", {
    tanks = {
        principal = { capacite = 1000.0, carburant = 800.0 },  -- en litres
        secours   = { capacite = 200.0,  carburant = 200.0 },
    },
    thrusters = {
        main = {
            force_max    = 50.0,   -- delta-v max par tick (km/jour)
            consommation = 0.01,   -- litres consommés par unité de force appliquée
            reservoir    = "principal",  -- tank à débiter
        },
        rcs = {
            force_max    = 5.0,
            consommation = 0.005,
            reservoir    = "principal",
        },
    },
    sensors = {
        radar = { portee = 50000.0 },  -- portée en km
    },
})
```

#### `get_fuel(ship_id, tank_id)` → `number`

Retourne le carburant restant dans un réservoir (0 si inconnu).

```lua
local restant = get_fuel("shp", "principal")
```

#### `empty_tank(ship_id, tank_id, volume)` → `bool`

Retire `volume` litres du réservoir. Retourne `true` si le retrait a réussi, `false` si le réservoir est insuffisant ou inexistant. Le réservoir n'est pas modifié en cas d'échec.

```lua
local ok = empty_tank("shp", "principal", 50.0)
if not ok then
    -- pas assez de carburant
end
```

#### `use_thruster(ship_id, thruster_id, power, direction)` → `bool`

Applique une poussée avec le moteur `thruster_id` :
- `power` : fraction de la puissance maximale (0.0 à 1.0)
- `direction` : table `{ x, y, z }` — sera normalisée automatiquement

Consomme du carburant dans le réservoir associé (`consommation × force_max × power` litres).
Retourne `false` si pas assez de carburant ou moteur inconnu, et n'applique pas la poussée.

```lua
local ok = use_thruster("shp", "main", 0.5, { x = 1.0, y = 0.0, z = 0.0 })
```

#### `detect_obstacle(data, sensor_id)` → `list`

Détecte tous les objets (corps célestes et vaisseaux) à portée du capteur `sensor_id`.
Retourne une liste triée par distance croissante.

**Structure d'un élément :**
```lua
{
    id       = "terre",      -- identifiant de l'objet
    type     = "corps",      -- "corps" ou "vaisseau"
    distance = 12345.6,      -- distance en km
}
```

```lua
local obstacles = detect_obstacle(data, "radar")
for _, obj in ipairs(obstacles) do
    if obj.type == "corps" and obj.distance < 5000 then
        fire("pause_game", {})
        return
    end
end
```

---

### 1.5 Exemple complet — Script de comportement

```lua
-- src/scripts/events/shp_comportement.lua
-- Vaisseau "shp" : pause si un corps est trop proche, propulsion manuelle.

declare_components("shp", {
    tanks = {
        principal = { capacite = 1000.0, carburant = 800.0 },
    },
    thrusters = {
        main = { force_max = 50.0, consommation = 0.01, reservoir = "principal" },
    },
    sensors = {
        radar = { portee = 100000.0 },  -- 100 000 km
    },
})

local SEUIL_DANGER = 10000.0  -- km
local etait_en_danger = {}    -- état par vaisseau : nil = premier tick

on("ship_tick", function(data)
    if data.ship_id ~= "shp" then return end

    -- Détection de proximité
    local obstacles = detect_obstacle(data, "radar")
    local en_danger = false
    for _, obj in ipairs(obstacles) do
        if obj.distance < SEUIL_DANGER then
            en_danger = true
            break
        end
    end

    -- Pause seulement au franchissement (pas si déjà en danger au démarrage)
    if etait_en_danger[data.ship_id] == false and en_danger then
        fire("pause_game", {})
    end
    etait_en_danger[data.ship_id] = en_danger

    -- Exemple de poussée automatique vers +X
    -- use_thruster("shp", "main", 0.1, { x = 1.0, y = 0.0, z = 0.0 })
end)
```

---

## 2. Scripts de propulsion (`src/scripts/ships/`)

Un script de propulsion est associé à un vaisseau par son nom : `src/scripts/ships/<ship_id>.lua`.
Il est exécuté à **chaque tick de simulation**, dans un contexte sans état (les variables locales ne persistent pas entre les ticks).

### 2.1 Globals disponibles

#### `ship`

Table décrivant le vaisseau courant :

```lua
ship.id           -- string : identifiant du vaisseau
ship.position     -- vec3   : position en km
ship.velocity     -- vec3   : vitesse en km/jour
```

#### `body(id)` → `table | nil`

Retourne les données d'un corps céleste, ou `nil` s'il n'existe pas.

```lua
local terre = body("terre")
if terre then
    terre.id        -- string
    terre.position  -- vec3
    terre.velocity  -- vec3
end
```

### 2.2 Fonctions utilitaires

#### `vec3(x, y, z)` → `vec3`

Construit une table vecteur 3D.

```lua
local v = vec3(1.0, 0.0, 0.0)
```

#### `length(v)` → `number`

Retourne la longueur d'un vecteur.

```lua
local d = length(ship.velocity)
```

#### `distance(a, b)` → `number`

Retourne la distance entre deux vecteurs.

```lua
local d = distance(ship.position, body("terre").position)
```

#### `normalize(v)` → `vec3`

Retourne le vecteur normalisé (longueur 1). Retourne un vecteur nul si `v` est nul.

```lua
local dir = normalize(vec3(3.0, 4.0, 0.0))  -- → {0.6, 0.8, 0.0}
```

#### `apply_global_thrust(v)`

Applique une impulsion `v` (km/jour) au vaisseau ce tick. Peut être appelé plusieurs fois ; les impulsions s'additionnent.

```lua
apply_global_thrust(vec3(0.0, 10.0, 0.0))
```

> Cette fonction ne consomme pas de carburant. Pour une simulation réaliste, gère le carburant dans un script de comportement et tire `apply_thrust` via `fire(...)`.

---

### 2.3 Exemple complet — Script de propulsion

```lua
-- src/scripts/ships/shp.lua
-- Freinage orbital : pousse dans la direction opposée à la vitesse si trop vite.

local VITESSE_MAX = 50.0  -- km/jour

local v = length(ship.velocity)
if v > VITESSE_MAX then
    local frein = normalize(vec3(-ship.velocity.x, -ship.velocity.y, -ship.velocity.z))
    apply_global_thrust(vec3(
        frein.x * 5.0,
        frein.y * 5.0,
        frein.z * 5.0
    ))
end
```

---

## 3. Communication entre scripts

Les scripts de comportement dans `src/scripts/events/` partagent le même bus d'événements et la même VM Lua. Ils peuvent donc communiquer via `fire` / `on` :

```lua
-- script_a.lua
on("ship_tick", function(data)
    if condition then
        fire("alerte_collision", { ship_id = data.ship_id, cible = "terre" })
    end
end)

-- script_b.lua
on("alerte_collision", function(data)
    -- réagit à l'alerte envoyée par script_a
    fire("pause_game", {})
end)
```

> Les événements tirés par un handler sont traités au **frame suivant** par les autres handlers Lua. Cependant, les événements système (`pause_game`, `apply_thrust`, etc.) sont transmis au moteur dans le **même frame**.

---

## 4. Référence rapide

### Événements reçus

| Événement | Données | Fréquence |
|---|---|---|
| `ship_tick` | `{ ship_id, bodies, ships }` | Chaque tick de simulation, par vaisseau |

### Événements système (émis vers le moteur)

| Événement | Données | Effet |
|---|---|---|
| `pause_game` | `{}` | Pause la simulation |
| `resume_game` | `{}` | Reprend la simulation |
| `apply_thrust` | `{ ship_id, dx, dy, dz }` | Applique un delta-v en km/jour |

### Fonctions de la bibliothèque composants

| Fonction | Retour | Description |
|---|---|---|
| `declare_components(ship_id, config)` | — | Initialise les composants d'un vaisseau |
| `get_fuel(ship_id, tank_id)` | `number` | Carburant restant en litres |
| `empty_tank(ship_id, tank_id, volume)` | `bool` | Vide un réservoir, retourne faux si insuffisant |
| `use_thruster(ship_id, thruster_id, power, dir)` | `bool` | Pousse + consomme carburant |
| `detect_obstacle(data, sensor_id)` | `list` | Objets à portée, triés par distance |

### Fonctions des scripts de propulsion

| Fonction | Retour | Description |
|---|---|---|
| `body(id)` | `table\|nil` | Données d'un corps céleste |
| `vec3(x, y, z)` | `vec3` | Construit un vecteur |
| `length(v)` | `number` | Norme du vecteur |
| `distance(a, b)` | `number` | Distance entre deux points |
| `normalize(v)` | `vec3` | Vecteur normalisé |
| `apply_global_thrust(v)` | — | Applique un delta-v ce tick |

### Globals des scripts de propulsion

| Global | Type | Description |
|---|---|---|
| `ship.id` | `string` | Identifiant du vaisseau |
| `ship.position` | `vec3` | Position en km |
| `ship.velocity` | `vec3` | Vitesse en km/jour |
