from torch.nn import Module


def frozen(net: Module) -> Module:
    return net.eval()


def scored(net: Module) -> Module:
    return net
