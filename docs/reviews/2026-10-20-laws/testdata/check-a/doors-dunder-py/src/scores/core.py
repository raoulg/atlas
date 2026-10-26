class Scorer:
    def __init__(self, weights: dict[str, float]) -> None:
        self.weights = weights

    def __call__(self, features):
        return sum(self.weights[k] * v for k, v in features.items())

    def __getitem__(self, key):
        return self.weights[key]
